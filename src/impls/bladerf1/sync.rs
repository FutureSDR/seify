use super::common::*;
use crate::{
    AgcControl, AntennaControl, Args, BandwidthControl, Capability, DeviceInfo, Direction,
    DriverError, Error, FrequencyControl, GainControl, Range, RxDevice, SampleRateControl,
    TxDevice,
};
use libbladerf_rs::bladerf1::hardware::lms6002d::dc_calibration::DcCalModule;
use libbladerf_rs::bladerf1::{
    BladeRf1, ExpansionBoard, GainDb, GainMode, RfLinkSession, RxStream, SampleFormat, TuningMode,
    TxStream,
};
use libbladerf_rs::Channel;
use libbladerf_rs::MaybeFuture;
use num_complex::Complex32;
#[cfg(target_os = "linux")]
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// bladeRF 1 device backend.
#[derive(Clone)]
pub struct BladeRf {
    inner: Arc<Mutex<BladeRf1>>,
}

impl BladeRf {
    #[allow(missing_docs)]
    pub fn from_shared_device(inner: Arc<Mutex<BladeRf1>>) -> Result<Self, Error> {
        {
            let mut dev = inner.lock().map_err(|_| Error::Busy)?;
            dev.rf_link_session()
                .wait()
                .map_err(bladerf_err)?
                .initialize(false)
                .wait()
                .map_err(bladerf_err)?;
        }
        Ok(Self { inner })
    }

    fn init_and_wrap(mut bladerf: BladeRf1) -> Result<Self, Error> {
        let mut session = bladerf.rf_link_session().wait().map_err(bladerf_err)?;
        session.initialize(false).wait().map_err(bladerf_err)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(bladerf)),
        })
    }

    /// Return descriptors for detected bladeRF 1 devices.
    #[cfg(not(target_os = "android"))]
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let selector = device_selector(args)?;
        let descriptors = BladeRf1::list_bladerf1()
            .wait()
            .map_err(|_| Error::DeviceNotFound)?
            .map(|info| probe_descriptor(&info))
            .collect();
        Ok(filter_descriptors(&selector, descriptors))
    }

    /// Returns no descriptors on Android, which requires [`Self::from_fd`].
    #[cfg(target_os = "android")]
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        Ok(Vec::new())
    }

    /// Open a bladeRF 1 device from arguments.
    #[cfg(not(target_os = "android"))]
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args: Args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        log::trace!("args: {args:?}");
        let bladerf = match device_selector(&args)? {
            #[cfg(target_os = "linux")]
            DeviceSelector::Fd(fd) => {
                let fd = unsafe { OwnedFd::from_raw_fd(fd) };
                BladeRf1::from_fd(fd).wait()
            }
            DeviceSelector::BusAddr(bus_id, address) => {
                BladeRf1::from_bus_addr(&bus_id, address).wait()
            }
            DeviceSelector::Serial(serial) => BladeRf1::from_serial(&serial).wait(),
            DeviceSelector::First => {
                log::trace!("Opening first bladerf device");
                BladeRf1::from_first().wait()
            }
        }
        .map_err(bladerf_err)?;
        Self::init_and_wrap(bladerf)
    }

    /// Reports that Android opening requires [`Self::from_fd`].
    ///
    /// # Errors
    /// Returns an unsupported-operation error because Android cannot enumerate USB devices.
    #[cfg(target_os = "android")]
    pub fn open<A: TryInto<Args>>(_args: A) -> Result<Self, Error> {
        Err(Error::unsupported_reason(
            Capability::DriverOperation,
            "Android requires BladeRf::from_fd with an owned USB connection",
        ))
    }

    /// Opens a bladeRF 1 from an owned USB file descriptor.
    ///
    /// Android applications obtain USB permission before calling this constructor.
    /// Duplicate the descriptor first if Java retains its connection ownership.
    ///
    /// # Examples
    /// ```no_run
    /// # fn open(fd: std::os::fd::OwnedFd) -> Result<seify::DynDevice, seify::Error> {
    /// let backend = seify::impls::BladeRf::from_fd(fd)?;
    /// Ok(seify::DynDevice::from_impl(backend))
    /// # }
    /// ```
    ///
    /// # Errors
    /// Propagates USB opening, interface claiming, and initialization failures.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub fn from_fd(fd: std::os::fd::OwnedFd) -> Result<Self, Error> {
        Self::init_and_wrap(BladeRf1::from_fd(fd).wait().map_err(bladerf_err)?)
    }

    /// Attach and enable a bladeRF expansion board.
    pub fn enable_expansion_board(&mut self, board_type: ExpansionBoard) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        session
            .expansion_attach(board_type)
            .wait()
            .map_err(bladerf_err)
    }

    /// Run DC calibration for the selected calibration module.
    pub fn calibrate_dc(&mut self, module: DcCalModule) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        session.calibrate_dc(module).wait().map_err(bladerf_err)
    }
}

/// bladeRF 1 receive streamer.
pub struct RxStreamer {
    streamer: Option<RxStream>,
    dev: Arc<Mutex<BladeRf1>>,
    converter: RxConverter,
}

/// bladeRF 1 transmit streamer.
pub struct TxStreamer {
    streamer: Option<TxStream>,
    dev: Arc<Mutex<BladeRf1>>,
    format: SampleFormat,
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(BUFFER_SIZE / STREAM_FORMAT.sample_size())
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        let mut dev = self.dev.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        match self
            .streamer
            .as_mut()
            .ok_or(Error::StreamInactive)?
            .start(&mut session)
            .wait()
        {
            Ok(()) | Err(libbladerf_rs::Error::StreamAlreadyStarted) => Ok(()),
            Err(error) => Err(bladerf_err(error)),
        }
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        let streamer = self.streamer.as_mut().ok_or(Error::StreamClosed)?;
        if let Some(buffer) = self.converter.take_pending() {
            streamer.recycle(buffer);
        }
        let mut dev = self.dev.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        match streamer.stop(&mut session).wait() {
            Ok(()) | Err(libbladerf_rs::Error::StreamNotStarted) => Ok(()),
            Err(error) => Err(bladerf_err(error)),
        }
    }

    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        let streamer = self.streamer.as_mut().ok_or(Error::StreamInactive)?;
        let output = &mut buffers[0];

        let (mut written, recycled) = self.converter.drain_pending(output)?;
        if let Some(buf) = recycled {
            streamer.recycle(buf);
        }
        if written >= output.len() {
            return Ok(written);
        }

        let dma_buffer = streamer
            .read(Some(Duration::from_micros(timeout_us as u64)))
            .wait()
            .map_err(bladerf_err)?;
        let (n, recycled) = self.converter.consume(dma_buffer, &mut output[written..])?;
        if let Some(buf) = recycled {
            streamer.recycle(buf);
        }
        written += n;
        Ok(written)
    }
}

impl Drop for RxStreamer {
    fn drop(&mut self) {
        if let Some(streamer) = self.streamer.as_mut() {
            if let Some(buffer) = self.converter.take_pending() {
                streamer.recycle(buffer);
            }
            if let Ok(mut dev) = self.dev.lock() {
                if let Ok(mut session) = dev.rf_link_session().wait() {
                    let _ = streamer.close(&mut session).wait();
                }
            }
        }
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(BUFFER_SIZE / self.format.sample_size())
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        let mut dev = self.dev.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        match self
            .streamer
            .as_mut()
            .ok_or(Error::StreamInactive)?
            .start(&mut session)
            .wait()
        {
            Ok(()) | Err(libbladerf_rs::Error::StreamAlreadyStarted) => Ok(()),
            Err(error) => Err(bladerf_err(error)),
        }
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        let streamer = self.streamer.as_mut().ok_or(Error::StreamClosed)?;
        let mut dev = self.dev.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        match streamer.stop(&mut session).wait() {
            Ok(()) | Err(libbladerf_rs::Error::StreamNotStarted) => Ok(()),
            Err(error) => Err(bladerf_err(error)),
        }
    }

    fn write(
        &mut self,
        buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        let streamer = self.streamer.as_mut().ok_or(Error::StreamInactive)?;
        let bytes_per_sample = self.format.sample_size();
        let max_samples = BUFFER_SIZE / bytes_per_sample;
        let samples_to_write = buffers[0].len().min(max_samples);
        let bytes_needed = samples_to_write * bytes_per_sample;

        let mut dma_buffer = streamer
            .get_buffer(Some(Duration::from_micros(timeout_us as u64)))
            .wait()
            .map_err(bladerf_err)?;
        dma_buffer.clear();
        let converted = convert_complex32_to_bytes(
            self.format,
            &buffers[0][..samples_to_write],
            dma_buffer.extend_fill(bytes_needed, 0),
        )?;
        if converted != samples_to_write {
            streamer.recycle(dma_buffer);
            return Err(Error::Driver(DriverError::Other(
                "sample conversion produced a short TX buffer".into(),
            )));
        }
        streamer
            .submit(dma_buffer, bytes_needed)
            .map_err(bladerf_err)?;
        Ok(samples_to_write)
    }

    fn write_all(
        &mut self,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        _end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        let mut offset = 0;
        while offset < buffers[0].len() {
            let samples = &buffers[0][offset..];
            let written = self.write(
                &[samples],
                if offset == 0 { at_ns } else { None },
                false,
                timeout_us,
            )?;
            offset += written;
        }
        Ok(())
    }
}

impl Drop for TxStreamer {
    fn drop(&mut self) {
        if let Some(streamer) = self.streamer.as_mut() {
            if let Ok(mut dev) = self.dev.lock() {
                if let Ok(mut session) = dev.rf_link_session().wait() {
                    let _ = streamer.close(&mut session).wait();
                }
            }
        }
    }
}

impl BladeRf {
    fn driver(&self) -> crate::Driver {
        crate::Driver::BladeRf
    }

    fn id(&self) -> Result<String, Error> {
        self.inner
            .lock()
            .unwrap()
            .serial()
            .wait()
            .map_err(bladerf_err)
    }

    fn info(&self) -> Result<Args, Error> {
        let mut args = Args::default();
        args.set(
            "firmware version",
            self.inner
                .lock()
                .unwrap()
                .fx3_firmware_version()
                .wait()
                .map_err(bladerf_err)?,
        );
        Ok(args)
    }

    fn num_channels(&self, _: Direction) -> Result<usize, Error> {
        Ok(1)
    }

    fn full_duplex(&self) -> Result<bool, Error> {
        Ok(true)
    }

    fn antennas(&self, _direction: Direction, _channel: usize) -> Result<Vec<String>, Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    fn antenna(&self, _direction: Direction, _channel: usize) -> Result<String, Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    fn set_antenna(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<(), Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(ch(direction, channel)? == Channel::Rx)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        let channel = ch(direction, channel)?;
        if channel != Channel::Rx {
            return Err(Error::unsupported(Capability::Agc));
        }
        let mode = if agc {
            GainMode::Default
        } else {
            GainMode::Mgc
        };
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        session
            .set_gain_mode(channel, mode)
            .wait()
            .map_err(bladerf_err)
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        if ch(direction, channel)? != Channel::Rx {
            return Err(Error::unsupported(Capability::Agc));
        }
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        Ok(session.get_gain_mode().wait().map_err(bladerf_err)? == GainMode::Default)
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        Ok(RfLinkSession::get_gain_stages(ch(direction, channel)?)
            .iter()
            .map(|s| <&str>::from(*s).to_string())
            .collect())
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        let channel = ch(direction, channel)?;
        let range = RfLinkSession::get_gain_range(channel);
        let min = range.min().unwrap_or(f64::MIN);
        let max = range.max().unwrap_or(f64::MAX);
        let clamped = gain.clamp(min, max);
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        session
            .set_gain(channel, GainDb::from(clamped as i8))
            .wait()
            .map_err(bladerf_err)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        Ok(Some(
            session
                .get_gain(ch(direction, channel)?)
                .wait()
                .map_err(bladerf_err)?
                .db() as f64,
        ))
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        RfLinkSession::get_gain_range(ch(direction, channel)?).try_into()
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let stage = gain_stage(direction, channel, name)?;
        let range = RfLinkSession::get_gain_stage_range(stage);
        let min = range.min().unwrap_or(f64::MIN);
        let max = range.max().unwrap_or(f64::MAX);
        let clamped = gain.clamp(min, max);
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        session
            .set_gain_stage(stage, GainDb::from(clamped as i8))
            .wait()
            .map_err(bladerf_err)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        let stage = gain_stage(direction, channel, name)?;
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        Ok(Some(
            session
                .get_gain_stage(stage)
                .wait()
                .map_err(bladerf_err)?
                .db() as f64,
        ))
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let stage = gain_stage(direction, channel, name)?;
        RfLinkSession::get_gain_stage_range(stage).try_into()
    }

    fn frequency_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        session
            .get_frequency_range()
            .wait()
            .map_err(bladerf_err)?
            .try_into()
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        Ok(session
            .get_frequency(ch(direction, channel)?)
            .wait()
            .map_err(bladerf_err)? as f64)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        let f_range = session.get_frequency_range().wait().map_err(bladerf_err)?;
        if frequency < f_range.min().unwrap() {
            log::trace!("Frequency {frequency} requires XB200 expansion board");
            if session
                .expansion_get_attached()
                .wait()
                .map_err(bladerf_err)?
                != ExpansionBoard::Xb200
            {
                log::debug!("Automatically attaching XB200 expansion board");
                session
                    .expansion_attach(ExpansionBoard::Xb200)
                    .wait()
                    .map_err(bladerf_err)?;
            }
        }
        log::trace!("Setting frequency to {frequency}");
        let ch = ch(direction, channel)?;
        if session
            .set_frequency(ch, frequency as u64, TuningMode::Fpga)
            .wait()
            .is_err()
        {
            log::warn!("FPGA retune failed, falling back to host tuning");
            session
                .set_frequency(ch, frequency as u64, TuningMode::Host)
                .wait()
                .map_err(bladerf_err)?;
        }
        Ok(())
    }

    fn frequency_components(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Vec<String>, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    fn component_frequency_range(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<Range, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    fn component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<f64, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    fn set_component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
        _frequency: f64,
    ) -> Result<(), Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        Ok(session
            .get_sample_rate(ch(direction, channel)?)
            .wait()
            .map_err(bladerf_err)? as f64)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        let ch = ch(direction, channel)?;
        let actual = session
            .set_sample_rate(ch, rate as u32)
            .wait()
            .map_err(bladerf_err)?;
        if actual != rate as u32 {
            log::debug!("Requested sample rate {rate}, actual {actual}");
        }
        let bw_actual = session
            .set_bandwidth(ch, actual)
            .wait()
            .map_err(bladerf_err)?;
        if bw_actual != actual {
            log::debug!("Auto-set bandwidth to {bw_actual} (requested {actual})");
        }
        Ok(())
    }

    fn get_sample_rate_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        RfLinkSession::get_sample_rate_range().try_into()
    }

    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        Ok(session
            .get_bandwidth(ch(direction, channel)?)
            .wait()
            .map_err(bladerf_err)? as f64)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        let actual = session
            .set_bandwidth(ch(direction, channel)?, bw as u32)
            .wait()
            .map_err(bladerf_err)?;
        if actual != bw as u32 {
            log::debug!("Requested bandwidth {bw}, actual {actual}");
        }
        Ok(())
    }

    fn get_bandwidth_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        RfLinkSession::get_bandwidth_range().try_into()
    }
}

impl DeviceInfo for BladeRf {
    fn driver(&self) -> crate::Driver {
        BladeRf::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        BladeRf::id(self)
    }

    fn info(&self) -> Result<Args, Error> {
        BladeRf::info(self)
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        BladeRf::num_channels(self, direction)
    }

    fn full_duplex(&self) -> Result<bool, Error> {
        BladeRf::full_duplex(self)
    }
}

crate::impl_dyn_device_backend!(
    BladeRf => [rx, tx, antenna, agc, gain, frequency, sample_rate, bandwidth]
);
crate::registry::impl_typed_device_backend!(BladeRf, crate::Driver::BladeRf);

impl RxDevice for BladeRf {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        check_channels(channels, "RX")?;
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        let streamer = RxStream::builder(&mut session)
            .buffer_size(BUFFER_SIZE)
            .buffer_count(BUFFER_COUNT)
            .format(STREAM_FORMAT)
            .build()
            .wait()
            .map_err(bladerf_err)?;
        Ok(RxStreamer {
            streamer: Some(streamer),
            dev: Arc::clone(&self.inner),
            converter: RxConverter::new(STREAM_FORMAT),
        })
    }
}

impl TxDevice for BladeRf {
    type TxStreamer = TxStreamer;

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        check_channels(channels, "TX")?;
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().wait().map_err(bladerf_err)?;
        let streamer = TxStream::builder(&mut session)
            .buffer_size(BUFFER_SIZE)
            .buffer_count(BUFFER_COUNT)
            .format(STREAM_FORMAT)
            .build()
            .wait()
            .map_err(bladerf_err)?;
        Ok(TxStreamer {
            streamer: Some(streamer),
            dev: Arc::clone(&self.inner),
            format: STREAM_FORMAT,
        })
    }
}

impl AntennaControl for BladeRf {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        BladeRf::antennas(self, direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        BladeRf::antenna(self, direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        BladeRf::set_antenna(self, direction, channel, name)
    }
}

impl AgcControl for BladeRf {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        BladeRf::agc_available(self, direction, channel)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        BladeRf::set_agc_enabled(self, direction, channel, agc)
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        BladeRf::agc_enabled(self, direction, channel)
    }
}

impl GainControl for BladeRf {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        BladeRf::gain_elements(self, direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        BladeRf::set_gain(self, direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        BladeRf::gain(self, direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BladeRf::gain_range(self, direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        BladeRf::set_gain_element(self, direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        BladeRf::gain_element(self, direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        BladeRf::gain_element_range(self, direction, channel, name)
    }
}

impl FrequencyControl for BladeRf {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BladeRf::frequency_range(self, direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        BladeRf::frequency(self, direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        BladeRf::set_frequency(self, direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        BladeRf::frequency_components(self, direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        BladeRf::component_frequency_range(self, direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        BladeRf::component_frequency(self, direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        BladeRf::set_component_frequency(self, direction, channel, name, frequency)
    }
}

impl SampleRateControl for BladeRf {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        BladeRf::sample_rate(self, direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        BladeRf::set_sample_rate(self, direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BladeRf::get_sample_rate_range(self, direction, channel)
    }
}

impl BandwidthControl for BladeRf {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        BladeRf::bandwidth(self, direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        BladeRf::set_bandwidth(self, direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BladeRf::get_bandwidth_range(self, direction, channel)
    }
}
