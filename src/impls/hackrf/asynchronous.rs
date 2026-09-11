use std::future::IntoFuture;

use hackrf_nusb::{Device as HackRfDevice, RxStream, TxStream};
use num_complex::Complex32;

use super::common::*;
#[cfg(target_arch = "wasm32")]
use crate::dev::WebUsbDeviceFilter;
use crate::Direction::*;
use crate::{
    async_compat::{timeout_from_micros, with_timeout, Shared, TimeoutResult},
    dev::AsyncTypedDeviceBackend,
    Args, AsyncAntennaControl, AsyncDeviceInfo, AsyncFrequencyControl, AsyncGainControl,
    AsyncRxDevice, AsyncSampleRateControl, AsyncTxDevice, Capability, Direction, Driver, Error,
    Range,
};

/// Asynchronous HackRF half-duplex RX/TX device backend.
#[derive(Clone)]
pub struct AsyncHackRf {
    session_slot: Shared<AsyncSlot<AsyncHalfDuplexSession>>,
    dropped_streams: Shared<DroppedStreams>,
    serial: u128,
}

/// A logical asynchronous HackRF receive stream.
///
/// A read automatically selects RX, stopping an active TX stream first.
pub struct AsyncHackRfRxStreamer {
    session_slot: Shared<AsyncSlot<AsyncHalfDuplexSession>>,
    dropped_streams: Shared<DroppedStreams>,
    active: bool,
}

/// A logical asynchronous HackRF transmit stream.
///
/// A write automatically selects TX, stopping an active RX stream first.
pub struct AsyncHackRfTxStreamer {
    session_slot: Shared<AsyncSlot<AsyncHalfDuplexSession>>,
    dropped_streams: Shared<DroppedStreams>,
    active: bool,
}

/// The Seify-only half-duplex policy. The core owns failed-stream cleanup and
/// replacement rules; Seify only remembers a pending physical `stop`.
struct AsyncHalfDuplexSession {
    device: Box<HackRfDevice>,
    rx_stream: Option<RxStream>,
    tx_stream: Option<TxStream>,
    phase: HalfDuplexPhase,
}

impl AsyncHalfDuplexSession {
    async fn stop_current(&mut self) -> Result<(), Error> {
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
                    .await
                    .map_err(map_hackrf_error)?;
            }
            Tx => {
                self.tx_stream
                    .as_mut()
                    .ok_or(Error::DeviceDisconnected)?
                    .stop()
                    .await
                    .map_err(map_hackrf_error)?;
            }
        }
        self.phase = HalfDuplexPhase::Off;
        Ok(())
    }

    async fn activate(&mut self, direction: Direction) -> Result<(), Error> {
        if self.phase == HalfDuplexPhase::Active(direction) {
            return Ok(());
        }

        self.stop_current().await?;
        self.phase = HalfDuplexPhase::NeedsStop(direction);
        let result = match direction {
            Rx => self
                .rx_stream
                .as_mut()
                .ok_or(Error::DeviceDisconnected)?
                .start()
                .await
                .map_err(map_hackrf_error),
            Tx => self
                .tx_stream
                .as_mut()
                .ok_or(Error::DeviceDisconnected)?
                .start()
                .await
                .map_err(map_hackrf_error),
        };
        result?;
        self.phase = HalfDuplexPhase::Active(direction);
        Ok(())
    }

    async fn stop_direction(&mut self, direction: Direction) -> Result<(), Error> {
        if matches!(
            self.phase,
            HalfDuplexPhase::Active(active) | HalfDuplexPhase::NeedsStop(active)
                if active == direction
        ) {
            self.stop_current().await?;
        }
        Ok(())
    }

    fn mark_stream_failed(&mut self, direction: Direction) {
        self.phase = HalfDuplexPhase::NeedsStop(direction);
    }

    async fn discard_stream(&mut self, direction: Direction) -> Result<(), Error> {
        // Close consumes the stream even on failure or cancellation. The core
        // retains any pending cleanup and rejects unsafe starts or replacement.
        if matches!(
            self.phase,
            HalfDuplexPhase::Active(active) | HalfDuplexPhase::NeedsStop(active) if active == direction
        ) {
            self.phase = HalfDuplexPhase::Off;
        }
        match direction {
            Rx => {
                if let Some(stream) = self.rx_stream.take() {
                    stream.close().await.map_err(map_hackrf_error)?;
                }
            }
            Tx => {
                if let Some(stream) = self.tx_stream.take() {
                    stream.close().await.map_err(map_hackrf_error)?;
                }
            }
        }
        Ok(())
    }

    async fn cleanup_dropped(&mut self, dropped: u8) -> Result<(), Error> {
        if dropped & DroppedStreams::RX != 0 {
            self.discard_stream(Rx).await?;
        }
        if dropped & DroppedStreams::TX != 0 {
            self.discard_stream(Tx).await?;
        }
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn drop_stream_best_effort(&mut self, direction: Direction) {
        if matches!(
            self.phase,
            HalfDuplexPhase::Active(active) | HalfDuplexPhase::NeedsStop(active) if active == direction
        ) {
            self.phase = HalfDuplexPhase::Off;
        }
        // Native driver drop finishes cleanup synchronously, even for an
        // asynchronously started stream. Do not leave TX running until reuse.
        match direction {
            Rx => drop(self.rx_stream.take()),
            Tx => drop(self.tx_stream.take()),
        }
    }
}

struct DroppedStreamCleanup<'a> {
    dropped_streams: &'a DroppedStreams,
    bits: u8,
}

impl<'a> DroppedStreamCleanup<'a> {
    fn take(dropped_streams: &'a DroppedStreams) -> Self {
        Self {
            dropped_streams,
            bits: dropped_streams.take(),
        }
    }

    fn bits(&self) -> u8 {
        self.bits
    }

    fn commit(&mut self) {
        self.bits = 0;
    }
}

impl Drop for DroppedStreamCleanup<'_> {
    fn drop(&mut self) {
        if self.bits != 0 {
            self.dropped_streams.restore(self.bits);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct AsyncSlot<T>(std::sync::Mutex<Option<T>>);

#[cfg(target_arch = "wasm32")]
struct AsyncSlot<T>(std::cell::RefCell<Option<T>>);

impl<T> AsyncSlot<T> {
    fn new(value: T) -> Self {
        Self(Default::default()).with_value(value)
    }

    fn with_value(self, value: T) -> Self {
        let result = self.put(value);
        debug_assert!(result.is_ok());
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn take(&self) -> Option<T> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    #[cfg(target_arch = "wasm32")]
    fn take(&self) -> Option<T> {
        self.0.borrow_mut().take()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn put(&self, value: T) -> Result<(), T> {
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_some() {
            Err(value)
        } else {
            *slot = Some(value);
            Ok(())
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn put(&self, value: T) -> Result<(), T> {
        let mut slot = self.0.borrow_mut();
        if slot.is_some() {
            Err(value)
        } else {
            *slot = Some(value);
            Ok(())
        }
    }
}

struct AsyncSlotLease<T> {
    slot: Shared<AsyncSlot<T>>,
    value: Option<T>,
}

impl<T> AsyncSlotLease<T> {
    fn acquire(slot: &Shared<AsyncSlot<T>>) -> Result<Self, Error> {
        let value = slot.take().ok_or(Error::Busy)?;
        Ok(Self {
            slot: Shared::clone(slot),
            value: Some(value),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn try_acquire(slot: &Shared<AsyncSlot<T>>) -> Option<Self> {
        Some(Self {
            slot: Shared::clone(slot),
            value: Some(slot.take()?),
        })
    }

    fn value_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("slot lease always owns a value")
    }

    fn value(&self) -> &T {
        self.value.as_ref().expect("slot lease always owns a value")
    }
}

impl<T> Drop for AsyncSlotLease<T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            let result = self.slot.put(value);
            debug_assert!(result.is_ok(), "async slot was unexpectedly occupied");
        }
    }
}

async fn lease_half_duplex(
    session_slot: &Shared<AsyncSlot<AsyncHalfDuplexSession>>,
    dropped_streams: &Shared<DroppedStreams>,
) -> Result<AsyncSlotLease<AsyncHalfDuplexSession>, Error> {
    let mut session = AsyncSlotLease::acquire(session_slot)?;
    let mut cleanup = DroppedStreamCleanup::take(dropped_streams);
    session.value_mut().cleanup_dropped(cleanup.bits()).await?;
    cleanup.commit();
    Ok(session)
}

impl AsyncHackRf {
    /// Return descriptors for detected HackRF devices asynchronously.
    ///
    /// Every descriptor contains a normalized decimal 128-bit serial. Zero
    /// represents a missing, invalid, or all-zero USB serial.
    pub async fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let devices = HackRfDevice::list().await.map_err(map_hackrf_error)?;
        probe_args(args, devices)
    }

    /// Open a HackRF device from arguments asynchronously.
    pub async fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = device_selector(&args)?;
        let (device, serial) = open_selected_device_async(selector).await?;

        Ok(Self {
            session_slot: Shared::new(AsyncSlot::new(AsyncHalfDuplexSession {
                device: Box::new(device),
                rx_stream: None,
                tx_stream: None,
                phase: HalfDuplexPhase::Off,
            })),
            dropped_streams: Shared::new(DroppedStreams::new()),
            serial,
        })
    }

    async fn lease_session(&self) -> Result<AsyncSlotLease<AsyncHalfDuplexSession>, Error> {
        lease_half_duplex(&self.session_slot, &self.dropped_streams).await
    }

    fn driver(&self) -> Driver {
        Driver::HackRf
    }

    async fn id(&self) -> Result<String, Error> {
        Ok(self.serial.to_string())
    }

    async fn info(&self) -> Result<Args, Error> {
        let session = self.lease_session().await?;
        let info = session.value().device.info();
        let mut args = Args::default();
        args.set("driver", "hackrf");
        args.set("serial", self.id().await?);
        args.set("board", info.board_name());
        args.set("firmware_version", info.firmware_version.clone());
        args.set("usb_api_version", format!("0x{:04x}", info.usb_api_version));
        Ok(args)
    }

    async fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        match direction {
            Rx => Ok(1),
            Tx => Ok(1),
        }
    }

    async fn full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }

    async fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(vec![ANTENNA_NAME.to_owned()])
    }

    async fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        check_channel(direction, channel)?;
        Ok(ANTENNA_NAME.to_owned())
    }

    async fn set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
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

    async fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(directional_gain_elements(direction))
    }

    async fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        check_channel(direction, channel)?;
        let range = directional_overall_gain_range(direction);
        if !range.contains(gain) {
            return Err(Error::out_of_range("gain", range, gain));
        }
        let mut session = self.lease_session().await?;
        match direction {
            Rx => {
                let gain = distribute_overall_gain(gain);
                session
                    .value_mut()
                    .device
                    .set_lna_gain_db(gain.lna_gain_db)
                    .await
                    .map_err(map_hackrf_error)?;
                session
                    .value_mut()
                    .device
                    .set_vga_gain_db(gain.vga_gain_db)
                    .await
                    .map_err(map_hackrf_error)?;
                session
                    .value_mut()
                    .device
                    .set_amp_enable(gain.amp_enabled)
                    .await
                    .map_err(map_hackrf_error)
            }
            Tx => {
                let (amp, vga) = distribute_tx_gain(gain);
                session
                    .value_mut()
                    .device
                    .set_tx_vga_gain_db(vga)
                    .await
                    .map_err(map_hackrf_error)?;
                session
                    .value_mut()
                    .device
                    .set_amp_enable(amp)
                    .await
                    .map_err(map_hackrf_error)
            }
        }
    }

    async fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        check_channel(direction, channel)?;
        let session = self.lease_session().await?;
        Ok(Some(directional_overall_gain(
            direction,
            session.value().device.config(),
        )))
    }

    async fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        Ok(directional_overall_gain_range(direction))
    }

    async fn set_gain_element(
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

        let mut session = self.lease_session().await?;
        match gain_type {
            GainType::Amp => session.value_mut().device.set_amp_enable(gain != 0.0).await,
            GainType::Lna => session.value_mut().device.set_lna_gain_db(gain as u8).await,
            GainType::Vga if direction == Tx => {
                session
                    .value_mut()
                    .device
                    .set_tx_vga_gain_db(gain as u8)
                    .await
            }
            GainType::Vga => session.value_mut().device.set_vga_gain_db(gain as u8).await,
        }
        .map_err(map_hackrf_error)
    }

    async fn gain_element(
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
        let session = self.lease_session().await?;
        Ok(Some(directional_gain_value(
            direction,
            gain_type,
            session.value().device.config(),
        )))
    }

    async fn gain_element_range(
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

    async fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.component_frequency_range(direction, channel, "TUNER")
            .await
    }

    async fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.component_frequency(direction, channel, "TUNER").await
    }

    async fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        self.set_component_frequency(direction, channel, "TUNER", frequency)
            .await
    }

    async fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(vec!["TUNER".to_owned()])
    }

    async fn component_frequency_range(
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

    async fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.component_frequency_range(direction, channel, name)
            .await?;
        let session = self.lease_session().await?;
        Ok(session.value().device.config().frequency_hz() as f64)
    }

    async fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        let range = self
            .component_frequency_range(direction, channel, name)
            .await?;
        if !range.contains(frequency) {
            return Err(Error::out_of_range("frequency", range, frequency));
        }
        let mut session = self.lease_session().await?;
        session
            .value_mut()
            .device
            .set_frequency_hz(frequency as u64)
            .await
            .map_err(map_hackrf_error)
    }

    async fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_channel(direction, channel)?;
        let session = self.lease_session().await?;
        Ok(session.value().device.config().sample_rate_hz() as f64)
    }

    async fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let range = self.get_sample_rate_range(direction, channel).await?;
        if !range.contains(rate) {
            return Err(Error::out_of_range("sample_rate", range, rate));
        }
        let mut session = self.lease_session().await?;
        session
            .value_mut()
            .device
            .set_sample_rate_hz(rate as u32)
            .await
            .map_err(map_hackrf_error)
    }

    async fn get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        Ok(sample_rate_range())
    }
}

impl AsyncDeviceInfo for AsyncHackRf {
    fn driver(&self) -> Driver {
        AsyncHackRf::driver(self)
    }

    async fn async_id(&self) -> Result<String, Error> {
        AsyncHackRf::id(self).await
    }

    async fn async_info(&self) -> Result<Args, Error> {
        AsyncHackRf::info(self).await
    }

    async fn async_num_channels(&self, direction: Direction) -> Result<usize, Error> {
        AsyncHackRf::num_channels(self, direction).await
    }

    async fn async_full_duplex(&self) -> Result<bool, Error> {
        AsyncHackRf::full_duplex(self).await
    }
}

crate::impl_dyn_async_device_backend!(
    AsyncHackRf => [rx, tx, antenna, gain, frequency, sample_rate]
);

impl AsyncRxDevice for AsyncHackRf {
    type RxStreamer = AsyncHackRfRxStreamer;

    async fn async_rx_streamer(
        &self,
        channels: &[usize],
        _args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "hackrf",
                "HackRF exposes only RX channel 0",
            ));
        }
        let mut session = self.lease_session().await?;
        if session.value().rx_stream.is_some() {
            return Err(Error::Busy);
        }
        let stream = session
            .value()
            .device
            .rx_stream()
            .map_err(map_hackrf_error)?;
        session.value_mut().rx_stream = Some(stream);
        Ok(AsyncHackRfRxStreamer::new(
            Shared::clone(&self.session_slot),
            Shared::clone(&self.dropped_streams),
        ))
    }
}

impl AsyncTxDevice for AsyncHackRf {
    type TxStreamer = AsyncHackRfTxStreamer;

    async fn async_tx_streamer(
        &self,
        channels: &[usize],
        _args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "hackrf",
                "HackRF exposes only TX channel 0",
            ));
        }
        let mut session = self.lease_session().await?;
        if session.value().tx_stream.is_some() {
            return Err(Error::Busy);
        }
        let stream = session
            .value()
            .device
            .tx_stream()
            .map_err(map_hackrf_error)?;
        session.value_mut().tx_stream = Some(stream);
        Ok(AsyncHackRfTxStreamer::new(
            Shared::clone(&self.session_slot),
            Shared::clone(&self.dropped_streams),
        ))
    }
}

impl AsyncAntennaControl for AsyncHackRf {
    async fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncHackRf::antennas(self, direction, channel).await
    }

    async fn async_antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        AsyncHackRf::antenna(self, direction, channel).await
    }

    async fn async_set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        AsyncHackRf::set_antenna(self, direction, channel, name).await
    }
}

impl AsyncGainControl for AsyncHackRf {
    async fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncHackRf::gain_elements(self, direction, channel).await
    }

    async fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        AsyncHackRf::set_gain(self, direction, channel, gain).await
    }

    async fn async_gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        AsyncHackRf::gain(self, direction, channel).await
    }

    async fn async_gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        AsyncHackRf::gain_range(self, direction, channel).await
    }

    async fn async_set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        AsyncHackRf::set_gain_element(self, direction, channel, name, gain).await
    }

    async fn async_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        AsyncHackRf::gain_element(self, direction, channel, name).await
    }

    async fn async_gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AsyncHackRf::gain_element_range(self, direction, channel, name).await
    }
}

impl AsyncFrequencyControl for AsyncHackRf {
    async fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncHackRf::frequency_range(self, direction, channel).await
    }

    async fn async_frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncHackRf::frequency(self, direction, channel).await
    }

    async fn async_set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        AsyncHackRf::set_frequency(self, direction, channel, frequency, args).await
    }

    async fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncHackRf::frequency_components(self, direction, channel).await
    }

    async fn async_component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AsyncHackRf::component_frequency_range(self, direction, channel, name).await
    }

    async fn async_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        AsyncHackRf::component_frequency(self, direction, channel, name).await
    }

    async fn async_set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        AsyncHackRf::set_component_frequency(self, direction, channel, name, frequency).await
    }
}

impl AsyncSampleRateControl for AsyncHackRf {
    async fn async_sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncHackRf::sample_rate(self, direction, channel).await
    }

    async fn async_set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        AsyncHackRf::set_sample_rate(self, direction, channel, rate).await
    }

    async fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncHackRf::get_sample_rate_range(self, direction, channel).await
    }
}

impl AsyncHackRfRxStreamer {
    fn new(
        session_slot: Shared<AsyncSlot<AsyncHalfDuplexSession>>,
        dropped_streams: Shared<DroppedStreams>,
    ) -> Self {
        Self {
            session_slot,
            dropped_streams,
            active: false,
        }
    }
}

impl crate::AsyncRxStreamer for AsyncHackRfRxStreamer {
    async fn mtu(&self) -> Result<usize, Error> {
        Ok(F32_RX_MTU)
    }

    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        let mut session = lease_half_duplex(&self.session_slot, &self.dropped_streams).await?;
        session.value_mut().activate(Rx).await?;
        self.active = true;
        Ok(())
    }

    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        let mut session = lease_half_duplex(&self.session_slot, &self.dropped_streams).await?;
        session.value_mut().stop_direction(Rx).await?;
        self.active = false;
        Ok(())
    }

    async fn read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        if buffers[0].is_empty() {
            return Ok(0);
        }

        let mut session = lease_half_duplex(&self.session_slot, &self.dropped_streams).await?;
        session.value_mut().activate(Rx).await?;
        let out = &mut buffers[0];
        let stream = session
            .value_mut()
            .rx_stream
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?;
        let result = match with_timeout(
            stream.read(out, None).into_future(),
            timeout_from_micros(timeout_us),
        )
        .await
        {
            TimeoutResult::Completed(read) => read.map_err(map_hackrf_error),
            TimeoutResult::TimedOut => Ok(0),
        };
        if result.is_err() {
            session.value_mut().mark_stream_failed(Rx);
        }
        result
    }
}

impl Drop for AsyncHackRfRxStreamer {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(mut session) = AsyncSlotLease::try_acquire(&self.session_slot) {
            session.value_mut().drop_stream_best_effort(Rx);
            return;
        }
        // The next session operation awaits close before claiming a replacement
        // or switching direction, including on WebUSB where drop is asynchronous.
        self.dropped_streams.request(Rx);
    }
}

impl AsyncHackRfTxStreamer {
    fn new(
        session_slot: Shared<AsyncSlot<AsyncHalfDuplexSession>>,
        dropped_streams: Shared<DroppedStreams>,
    ) -> Self {
        Self {
            session_slot,
            dropped_streams,
            active: false,
        }
    }

    async fn write_with_session(
        &mut self,
        session: &mut AsyncHalfDuplexSession,
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
        session.activate(Tx).await?;
        // hackrf-nusb owns the terminal boundary even if this awaited call is
        // cancelled after it accepts samples. Its async write ignores the
        // Seify timeout for the same reason.
        let _ = timeout_us;
        let result = session
            .tx_stream
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .write(buffers[0], None, end_burst)
            .await
            .map_err(map_hackrf_error);
        if result.is_err() {
            session.mark_stream_failed(Tx);
        }
        result
    }
}

impl crate::AsyncTxStreamer for AsyncHackRfTxStreamer {
    async fn mtu(&self) -> Result<usize, Error> {
        Ok(F32_TX_MTU)
    }

    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        let mut session = lease_half_duplex(&self.session_slot, &self.dropped_streams).await?;
        session.value_mut().activate(Tx).await?;
        self.active = true;
        Ok(())
    }

    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        let mut session = lease_half_duplex(&self.session_slot, &self.dropped_streams).await?;
        session.value_mut().stop_direction(Tx).await?;
        self.active = false;
        Ok(())
    }

    async fn write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        let session_slot = Shared::clone(&self.session_slot);
        let dropped_streams = Shared::clone(&self.dropped_streams);
        let mut session = lease_half_duplex(&session_slot, &dropped_streams).await?;
        self.write_with_session(session.value_mut(), buffers, at_ns, end_burst, timeout_us)
            .await
    }

    async fn write_all<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
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
        let session_slot = Shared::clone(&self.session_slot);
        let dropped_streams = Shared::clone(&self.dropped_streams);
        let mut session = lease_half_duplex(&session_slot, &dropped_streams).await?;
        let written = self
            .write_with_session(session.value_mut(), buffers, at_ns, end_burst, timeout_us)
            .await?;
        if written == buffers[0].len() {
            Ok(())
        } else {
            Err(Error::Timeout)
        }
    }
}

impl Drop for AsyncHackRfTxStreamer {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(mut session) = AsyncSlotLease::try_acquire(&self.session_slot) {
            session.value_mut().drop_stream_best_effort(Tx);
            return;
        }
        self.dropped_streams.request(Tx);
    }
}

impl AsyncTypedDeviceBackend for AsyncHackRf {
    fn driver() -> Driver {
        Driver::HackRf
    }

    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(args: &Args) -> Result<Vec<WebUsbDeviceFilter>, Error> {
        let serial = match device_selector(args)? {
            DeviceSelector::Serial(serial) if serial != 0 => Some(format!("{serial:032x}")),
            DeviceSelector::First | DeviceSelector::Serial(_) | DeviceSelector::Index(_) => None,
        };
        Ok([(0x1d50, 0x604b), (0x1d50, 0x6089), (0x1d50, 0xcc15)]
            .into_iter()
            .map(|(vendor_id, product_id)| {
                let filter = WebUsbDeviceFilter::new().with_vendor_product(vendor_id, product_id);
                if let Some(serial) = &serial {
                    filter.with_serial_number(serial.clone())
                } else {
                    filter
                }
            })
            .collect())
    }

    async fn async_probe(args: &Args) -> Result<Vec<Args>, Error> {
        Self::probe(args).await
    }

    async fn async_open(args: &Args) -> Result<Self, Error> {
        Self::open(args.clone()).await
    }
}

async fn open_selected_device_async(
    selector: DeviceSelector,
) -> Result<(HackRfDevice, u128), Error> {
    match selector {
        DeviceSelector::First | DeviceSelector::Serial(0) => HackRfDevice::open()
            .await
            .map(|device| {
                let serial = device.info().serial.normalized();
                (device, serial)
            })
            .map_err(map_hackrf_error),
        DeviceSelector::Serial(serial) => HackRfDevice::open_serial(serial)
            .await
            .map(|device| (device, serial))
            .map_err(map_hackrf_error),
        DeviceSelector::Index(index) => {
            let devices = HackRfDevice::list().await.map_err(map_hackrf_error)?;
            let Some(info) = devices.get(index) else {
                return Err(Error::DeviceNotFound);
            };
            match info.serial {
                Some(serial) => HackRfDevice::open_serial(serial)
                    .await
                    .map(|device| (device, serial))
                    .map_err(map_hackrf_error),
                None if index == 0 => HackRfDevice::open()
                    .await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "smol", feature = "tokio"))]
    #[test]
    fn dropping_async_slot_lease_returns_value() {
        let slot = Shared::new(AsyncSlot::new(7));

        let lease = AsyncSlotLease::acquire(&slot).expect("acquire slot lease");
        assert!(matches!(AsyncSlotLease::acquire(&slot), Err(Error::Busy)));
        drop(lease);

        let lease = AsyncSlotLease::acquire(&slot).expect("reacquire slot lease");
        assert_eq!(*lease.value.as_ref().expect("lease owns value"), 7);
    }

    #[test]
    fn dropped_stream_requests_are_coalesced_and_retryable() {
        let dropped = DroppedStreams::new();

        dropped.request(Rx);
        dropped.request(Tx);
        assert_eq!(dropped.take(), DroppedStreams::RX | DroppedStreams::TX);
        assert_eq!(dropped.take(), 0);

        dropped.restore(DroppedStreams::RX);
        assert_eq!(dropped.take(), DroppedStreams::RX);
    }

    #[test]
    fn cancelled_cleanup_preserves_pending_and_new_drop_requests() {
        let dropped = DroppedStreams::new();
        dropped.request(Rx);
        let cleanup = DroppedStreamCleanup::take(&dropped);
        assert_eq!(cleanup.bits(), DroppedStreams::RX);

        dropped.request(Tx);
        drop(cleanup);
        assert_eq!(dropped.take(), DroppedStreams::RX | DroppedStreams::TX);
    }

    #[test]
    fn completed_cleanup_preserves_only_new_drop_requests() {
        let dropped = DroppedStreams::new();
        dropped.request(Rx);
        let mut cleanup = DroppedStreamCleanup::take(&dropped);

        dropped.request(Tx);
        cleanup.commit();
        drop(cleanup);
        assert_eq!(dropped.take(), DroppedStreams::TX);
    }
}
