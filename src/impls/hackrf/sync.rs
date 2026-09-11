use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use hackrf_nusb::{Device as HackRfDevice, MaybeFuture, RxStream, TxStream};
use num_complex::Complex32;

use super::common::*;
use crate::Direction::*;
use crate::{
    AntennaControl, Args, Capability, DeviceInfo, Direction, Driver, Error, FrequencyControl,
    GainControl, Range, RxDevice, SampleRateControl, TxDevice,
};

/// HackRF half-duplex RX/TX device backend.
#[derive(Clone)]
pub struct HackRf {
    session: Arc<Mutex<HalfDuplexSession>>,
    dropped_streams: Arc<DroppedStreams>,
    serial: u128,
}

/// A logical HackRF receive stream.
///
/// A read automatically selects RX, stopping an active TX stream first.
pub struct RxStreamer {
    session: Arc<Mutex<HalfDuplexSession>>,
    dropped_streams: Arc<DroppedStreams>,
    active: bool,
}

/// A logical HackRF transmit stream.
///
/// A write automatically selects TX, stopping an active RX stream first.
pub struct TxStreamer {
    session: Arc<Mutex<HalfDuplexSession>>,
    dropped_streams: Arc<DroppedStreams>,
    active: bool,
}

/// The Seify-only half-duplex policy. The core owns failed-stream cleanup and
/// replacement rules; Seify only remembers a pending physical `stop`.
struct HalfDuplexSession {
    device: HackRfDevice,
    rx_stream: Option<RxStream>,
    tx_stream: Option<TxStream>,
    phase: HalfDuplexPhase,
}

impl HalfDuplexSession {
    fn stop_current(&mut self) -> Result<(), Error> {
        let direction = match self.phase {
            HalfDuplexPhase::Off => return Ok(()),
            HalfDuplexPhase::Active(direction) | HalfDuplexPhase::NeedsStop(direction) => direction,
        };
        self.phase = HalfDuplexPhase::NeedsStop(direction);
        match direction {
            Rx => {
                self.rx_stream
                    .as_mut()
                    .ok_or(Error::DeviceDisconnected)?
                    .stop()
                    .wait()
                    .map_err(map_hackrf_error)?;
            }
            Tx => {
                self.tx_stream
                    .as_mut()
                    .ok_or(Error::DeviceDisconnected)?
                    .stop()
                    .wait()
                    .map_err(map_hackrf_error)?;
            }
        }
        self.phase = HalfDuplexPhase::Off;
        Ok(())
    }

    fn activate(&mut self, direction: Direction) -> Result<(), Error> {
        if self.phase == HalfDuplexPhase::Active(direction) {
            return Ok(());
        }

        self.stop_current()?;
        self.phase = HalfDuplexPhase::NeedsStop(direction);
        let result = match direction {
            Rx => self
                .rx_stream
                .as_mut()
                .ok_or(Error::DeviceDisconnected)?
                .start()
                .wait()
                .map_err(map_hackrf_error),
            Tx => self
                .tx_stream
                .as_mut()
                .ok_or(Error::DeviceDisconnected)?
                .start()
                .wait()
                .map_err(map_hackrf_error),
        };
        result?;
        self.phase = HalfDuplexPhase::Active(direction);
        Ok(())
    }

    fn stop_direction(&mut self, direction: Direction) -> Result<(), Error> {
        if matches!(
            self.phase,
            HalfDuplexPhase::Active(active) | HalfDuplexPhase::NeedsStop(active) if active == direction
        ) {
            self.stop_current()?;
        }
        Ok(())
    }

    fn mark_stream_failed(&mut self, direction: Direction) {
        self.phase = HalfDuplexPhase::NeedsStop(direction);
    }

    fn discard_stream(&mut self, direction: Direction) -> Result<(), Error> {
        if matches!(
            self.phase,
            HalfDuplexPhase::Active(active) | HalfDuplexPhase::NeedsStop(active) if active == direction
        ) {
            self.phase = HalfDuplexPhase::Off;
        }
        match direction {
            Rx => {
                if let Some(stream) = self.rx_stream.take() {
                    stream.close().wait().map_err(map_hackrf_error)?;
                }
            }
            Tx => {
                if let Some(stream) = self.tx_stream.take() {
                    stream.close().wait().map_err(map_hackrf_error)?;
                }
            }
        }
        Ok(())
    }

    fn cleanup_dropped(&mut self, dropped: u8) -> Result<(), Error> {
        if dropped & DroppedStreams::RX != 0 {
            self.discard_stream(Rx)?;
        }
        if dropped & DroppedStreams::TX != 0 {
            self.discard_stream(Tx)?;
        }
        Ok(())
    }
}

fn lease_half_duplex<'a>(
    session: &'a Arc<Mutex<HalfDuplexSession>>,
    dropped_streams: &DroppedStreams,
) -> Result<MutexGuard<'a, HalfDuplexSession>, Error> {
    let mut session = session.lock().unwrap();
    let dropped = dropped_streams.take();
    if let Err(error) = session.cleanup_dropped(dropped) {
        dropped_streams.restore(dropped);
        return Err(error);
    }
    Ok(session)
}

trait HackRfDeviceControl {
    fn set_frequency_hz_sync(&mut self, frequency_hz: u64) -> Result<(), Error>;
    fn set_sample_rate_hz_sync(&mut self, sample_rate_hz: u32) -> Result<(), Error>;
    fn set_amp_enable_sync(&mut self, enabled: bool) -> Result<(), Error>;
    fn set_lna_gain_db_sync(&mut self, gain_db: u8) -> Result<(), Error>;
    fn set_vga_gain_db_sync(&mut self, gain_db: u8) -> Result<(), Error>;
    fn set_tx_vga_gain_db_sync(&mut self, gain_db: u8) -> Result<(), Error>;
}

impl HackRfDeviceControl for HackRfDevice {
    fn set_frequency_hz_sync(&mut self, frequency_hz: u64) -> Result<(), Error> {
        HackRfDevice::set_frequency_hz(self, frequency_hz)
            .wait()
            .map_err(map_hackrf_error)
    }

    fn set_sample_rate_hz_sync(&mut self, sample_rate_hz: u32) -> Result<(), Error> {
        HackRfDevice::set_sample_rate_hz(self, sample_rate_hz)
            .wait()
            .map_err(map_hackrf_error)
    }

    fn set_amp_enable_sync(&mut self, enabled: bool) -> Result<(), Error> {
        HackRfDevice::set_amp_enable(self, enabled)
            .wait()
            .map_err(map_hackrf_error)
    }

    fn set_lna_gain_db_sync(&mut self, gain_db: u8) -> Result<(), Error> {
        HackRfDevice::set_lna_gain_db(self, gain_db)
            .wait()
            .map_err(map_hackrf_error)
    }

    fn set_vga_gain_db_sync(&mut self, gain_db: u8) -> Result<(), Error> {
        HackRfDevice::set_vga_gain_db(self, gain_db)
            .wait()
            .map_err(map_hackrf_error)
    }

    fn set_tx_vga_gain_db_sync(&mut self, gain_db: u8) -> Result<(), Error> {
        HackRfDevice::set_tx_vga_gain_db(self, gain_db)
            .wait()
            .map_err(map_hackrf_error)
    }
}

impl HackRf {
    /// Return descriptors for detected HackRF devices.
    ///
    /// Every descriptor contains a normalized decimal 128-bit serial. Zero
    /// represents a missing, invalid, or all-zero USB serial.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let devices = HackRfDevice::list().wait().map_err(map_hackrf_error)?;
        probe_args(args, devices)
    }

    /// Open a HackRF device from arguments.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = device_selector(&args)?;
        let (device, serial) = open_selected_device(selector)?;

        Ok(Self {
            session: Arc::new(Mutex::new(HalfDuplexSession {
                device,
                rx_stream: None,
                tx_stream: None,
                phase: HalfDuplexPhase::Off,
            })),
            dropped_streams: Arc::new(DroppedStreams::new()),
            serial,
        })
    }

    fn lease_session(&self) -> Result<MutexGuard<'_, HalfDuplexSession>, Error> {
        lease_half_duplex(&self.session, &self.dropped_streams)
    }

    fn with_device<T>(
        &self,
        operation: impl FnOnce(&mut HackRfDevice) -> Result<T, Error>,
    ) -> Result<T, Error> {
        operation(&mut self.lease_session()?.device)
    }

    fn driver(&self) -> Driver {
        Driver::HackRf
    }

    fn id(&self) -> Result<String, Error> {
        Ok(self.serial.to_string())
    }

    fn info(&self) -> Result<Args, Error> {
        self.with_device(|device| {
            let info = device.info();
            let mut args = Args::default();
            args.set("driver", "hackrf");
            args.set("serial", self.id()?);
            args.set("board", info.board_name());
            args.set("firmware_version", info.firmware_version.clone());
            args.set("usb_api_version", format!("0x{:04x}", info.usb_api_version));
            Ok(args)
        })
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        match direction {
            Rx => Ok(1),
            Tx => Ok(1),
        }
    }

    fn full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(vec![ANTENNA_NAME.to_owned()])
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        check_channel(direction, channel)?;
        Ok(ANTENNA_NAME.to_owned())
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        check_channel(direction, channel)?;
        if name.eq_ignore_ascii_case(ANTENNA_NAME) {
            Ok(())
        } else {
            Err(Error::invalid_argument(
                "antenna",
                "HackRF exposes one antenna port named ANT",
            ))
        }
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(directional_gain_elements(direction))
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        check_channel(direction, channel)?;
        let range = directional_overall_gain_range(direction);
        if !range.contains(gain) {
            return Err(Error::out_of_range("gain", range, gain));
        }
        self.with_device(|device| match direction {
            Rx => {
                let gain = distribute_overall_gain(gain);
                device.set_lna_gain_db_sync(gain.lna_gain_db)?;
                device.set_vga_gain_db_sync(gain.vga_gain_db)?;
                device.set_amp_enable_sync(gain.amp_enabled)
            }
            Tx => {
                let (amp, vga) = distribute_tx_gain(gain);
                device.set_tx_vga_gain_db_sync(vga)?;
                device.set_amp_enable_sync(amp)
            }
        })
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        check_channel(direction, channel)?;
        self.with_device(|device| Ok(Some(directional_overall_gain(direction, device.config()))))
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        Ok(directional_overall_gain_range(direction))
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        check_channel(direction, channel)?;
        let gain_type = directional_gain_type(direction, name).ok_or(Error::invalid_argument(
            "hackrf",
            "invalid HackRF gain element",
        ))?;
        let range = directional_gain_range(direction, gain_type);
        if !range.contains(gain) {
            return Err(Error::out_of_range("gain", range, gain));
        }

        self.with_device(|device| match gain_type {
            GainType::Amp => device.set_amp_enable_sync(gain != 0.0),
            GainType::Lna => device.set_lna_gain_db_sync(gain as u8),
            GainType::Vga if direction == Tx => device.set_tx_vga_gain_db_sync(gain as u8),
            GainType::Vga => device.set_vga_gain_db_sync(gain as u8),
        })
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        check_channel(direction, channel)?;
        let gain_type = directional_gain_type(direction, name).ok_or(Error::invalid_argument(
            "hackrf",
            "invalid HackRF gain element",
        ))?;
        self.with_device(|device| {
            Ok(Some(directional_gain_value(
                direction,
                gain_type,
                device.config(),
            )))
        })
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        directional_gain_type(direction, name)
            .map(|gain| directional_gain_range(direction, gain))
            .ok_or(Error::invalid_argument(
                "hackrf",
                "invalid HackRF gain element",
            ))
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.component_frequency_range(direction, channel, "TUNER")
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.component_frequency(direction, channel, "TUNER")
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        self.set_component_frequency(direction, channel, "TUNER", frequency)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(vec!["TUNER".to_owned()])
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        if name == "TUNER" {
            Ok(frequency_range())
        } else {
            Err(Error::invalid_argument(
                "hackrf",
                "invalid HackRF frequency component",
            ))
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.component_frequency_range(direction, channel, name)?;
        self.with_device(|device| Ok(device.config().frequency_hz() as f64))
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        let range = self.component_frequency_range(direction, channel, name)?;
        if !range.contains(frequency) {
            return Err(Error::out_of_range("frequency", range, frequency));
        }
        self.with_device(|device| device.set_frequency_hz_sync(frequency as u64))
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_channel(direction, channel)?;
        self.with_device(|device| Ok(device.config().sample_rate_hz() as f64))
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let range = self.get_sample_rate_range(direction, channel)?;
        if !range.contains(rate) {
            return Err(Error::out_of_range("sample_rate", range, rate));
        }
        self.with_device(|device| device.set_sample_rate_hz_sync(rate as u32))
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        Ok(sample_rate_range())
    }
}

impl DeviceInfo for HackRf {
    fn driver(&self) -> Driver {
        HackRf::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        HackRf::id(self)
    }

    fn info(&self) -> Result<Args, Error> {
        HackRf::info(self)
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        HackRf::num_channels(self, direction)
    }

    fn full_duplex(&self) -> Result<bool, Error> {
        HackRf::full_duplex(self)
    }
}

crate::impl_dyn_device_backend!(
    HackRf => [rx, tx, antenna, gain, frequency, sample_rate]
);
crate::registry::impl_typed_device_backend!(HackRf, Driver::HackRf);

impl RxDevice for HackRf {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "hackrf",
                "HackRF exposes only RX channel 0",
            ));
        }
        let mut session = self.lease_session()?;
        if session.rx_stream.is_some() {
            return Err(Error::Busy);
        }
        let stream = session.device.rx_stream().map_err(map_hackrf_error)?;
        session.rx_stream = Some(stream);
        Ok(RxStreamer::new(
            Arc::clone(&self.session),
            Arc::clone(&self.dropped_streams),
        ))
    }
}

impl TxDevice for HackRf {
    type TxStreamer = TxStreamer;

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "hackrf",
                "HackRF exposes only TX channel 0",
            ));
        }
        let mut session = self.lease_session()?;
        if session.tx_stream.is_some() {
            return Err(Error::Busy);
        }
        let stream = session.device.tx_stream().map_err(map_hackrf_error)?;
        session.tx_stream = Some(stream);
        Ok(TxStreamer::new(
            Arc::clone(&self.session),
            Arc::clone(&self.dropped_streams),
        ))
    }
}

impl AntennaControl for HackRf {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        HackRf::antennas(self, direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        HackRf::antenna(self, direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        HackRf::set_antenna(self, direction, channel, name)
    }
}

impl GainControl for HackRf {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        HackRf::gain_elements(self, direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        HackRf::set_gain(self, direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        HackRf::gain(self, direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HackRf::gain_range(self, direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        HackRf::set_gain_element(self, direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        HackRf::gain_element(self, direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        HackRf::gain_element_range(self, direction, channel, name)
    }
}

impl FrequencyControl for HackRf {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HackRf::frequency_range(self, direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        HackRf::frequency(self, direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        HackRf::set_frequency(self, direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        HackRf::frequency_components(self, direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        HackRf::component_frequency_range(self, direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        HackRf::component_frequency(self, direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        HackRf::set_component_frequency(self, direction, channel, name, frequency)
    }
}

impl SampleRateControl for HackRf {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        HackRf::sample_rate(self, direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        HackRf::set_sample_rate(self, direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HackRf::get_sample_rate_range(self, direction, channel)
    }
}

impl RxStreamer {
    fn new(session: Arc<Mutex<HalfDuplexSession>>, dropped_streams: Arc<DroppedStreams>) -> Self {
        Self {
            session,
            dropped_streams,
            active: false,
        }
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(F32_RX_MTU)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        lease_half_duplex(&self.session, &self.dropped_streams)?.activate(Rx)?;
        self.active = true;
        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        lease_half_duplex(&self.session, &self.dropped_streams)?.stop_direction(Rx)?;
        self.active = false;
        Ok(())
    }

    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        if buffers[0].is_empty() {
            return Ok(0);
        }
        let mut session = lease_half_duplex(&self.session, &self.dropped_streams)?;
        session.activate(Rx)?;

        let out = &mut buffers[0];
        let read_len = out.len().min(F32_RX_MTU);
        let timeout = if timeout_us < 0 {
            None
        } else {
            Some(Duration::from_micros(timeout_us as u64))
        };
        let result = session
            .rx_stream
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .read(&mut out[..read_len], timeout)
            .wait()
            .map_err(map_hackrf_error);
        if result.is_err() {
            session.mark_stream_failed(Rx);
        }
        result
    }
}

impl Drop for RxStreamer {
    fn drop(&mut self) {
        if let Ok(mut session) = self.session.try_lock() {
            if session.discard_stream(Rx).is_ok() {
                return;
            }
        }
        self.dropped_streams.request(Rx);
    }
}

impl TxStreamer {
    fn new(session: Arc<Mutex<HalfDuplexSession>>, dropped_streams: Arc<DroppedStreams>) -> Self {
        Self {
            session,
            dropped_streams,
            active: false,
        }
    }

    fn write_with_session(
        &mut self,
        session: &mut HalfDuplexSession,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        if at_ns.is_some() {
            return Err(Error::unsupported_reason(
                Capability::DriverOperation,
                "timed HackRF TX is unsupported",
            ));
        }
        session.activate(Tx)?;
        let timeout = if timeout_us < 0 {
            None
        } else {
            Some(Duration::from_micros(timeout_us as u64))
        };
        let result = session
            .tx_stream
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .write(buffers[0], timeout, end_burst)
            .wait()
            .map_err(map_hackrf_error);
        if result.is_err() {
            session.mark_stream_failed(Tx);
        }
        result
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(F32_TX_MTU)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        lease_half_duplex(&self.session, &self.dropped_streams)?.activate(Tx)?;
        self.active = true;
        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        lease_half_duplex(&self.session, &self.dropped_streams)?.stop_direction(Tx)?;
        self.active = false;
        Ok(())
    }

    fn write(
        &mut self,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        let session_handle = Arc::clone(&self.session);
        let dropped_streams = Arc::clone(&self.dropped_streams);
        let mut session = lease_half_duplex(&session_handle, &dropped_streams)?;
        self.write_with_session(&mut session, buffers, at_ns, end_burst, timeout_us)
    }

    fn write_all(
        &mut self,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        if buffers[0].is_empty() && !end_burst {
            return Ok(());
        }
        if !self.active {
            return Err(Error::StreamInactive);
        }
        let session_handle = Arc::clone(&self.session);
        let dropped_streams = Arc::clone(&self.dropped_streams);
        let mut session = lease_half_duplex(&session_handle, &dropped_streams)?;
        let written =
            self.write_with_session(&mut session, buffers, at_ns, end_burst, timeout_us)?;
        if written == buffers[0].len() {
            Ok(())
        } else {
            Err(Error::Timeout)
        }
    }
}

impl Drop for TxStreamer {
    fn drop(&mut self) {
        if let Ok(mut session) = self.session.try_lock() {
            if session.discard_stream(Tx).is_ok() {
                return;
            }
        }
        self.dropped_streams.request(Tx);
    }
}

fn open_selected_device(selector: DeviceSelector) -> Result<(HackRfDevice, u128), Error> {
    match selector {
        DeviceSelector::First | DeviceSelector::Serial(0) => HackRfDevice::open()
            .wait()
            .map(|device| {
                let serial = device.info().serial.normalized();
                (device, serial)
            })
            .map_err(map_hackrf_error),
        DeviceSelector::Serial(serial) => HackRfDevice::open_serial(serial)
            .wait()
            .map(|device| (device, serial))
            .map_err(map_hackrf_error),
        DeviceSelector::Index(index) => {
            let devices = HackRfDevice::list().wait().map_err(map_hackrf_error)?;
            let Some(info) = devices.get(index) else {
                return Err(Error::DeviceNotFound);
            };
            match info.serial {
                Some(serial) => HackRfDevice::open_serial(serial)
                    .wait()
                    .map(|device| (device, serial))
                    .map_err(map_hackrf_error),
                None if index == 0 => HackRfDevice::open()
                    .wait()
                    .map(|device| (device, 0))
                    .map_err(map_hackrf_error),
                None => Err(Error::unsupported_reason(
                    Capability::DeviceId,
                    "cannot select a non-first HackRF that has no USB serial",
                )),
            }
        }
    }
}
