use super::{
    common::{
        buffer_samples, check_channel, map_error, named, numeric, probe_args, range, Metadata,
        Selector,
    },
    IioContext,
};
use crate::{Args, DeviceInfo, Direction, Driver, Error};
use crate::{Capability, Range};
use num_complex::Complex32;
use plutosdr::RxAttribute;
use plutosdr::{Device as PlutoDevice, MaybeFuture};
use std::sync::{Arc, Mutex};

/// Native PlutoSDR RX backend. Clones share one USB session.
#[derive(Clone)]
pub struct Pluto {
    session: Arc<Mutex<Option<PlutoDevice>>>,
    metadata: Arc<Metadata>,
}

impl Pluto {
    /// Enumerate standard Pluto USB devices, optionally selecting serial or index.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let selector = Selector::from_args(args)?;
        let devices = PlutoDevice::list().wait().map_err(map_error)?;
        Ok(selector
            .select(devices)
            .map(|(i, d)| probe_args(i, &d))
            .collect())
    }

    /// Open the named IIO interface and enable hardware DC tracking when supported.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = Selector::from_args(&args)?;
        let devices = PlutoDevice::list().wait().map_err(map_error)?;
        let (index, descriptor) = selector
            .select(devices)
            .next()
            .ok_or(Error::DeviceNotFound)?;
        let mut device = PlutoDevice::builder()
            .descriptor(descriptor)
            .open()
            .wait()
            .map_err(map_error)?;
        if device.dc_offset_available() {
            device
                .set_dc_offset_enabled(true)
                .wait()
                .map_err(map_error)?;
        }
        let metadata = Arc::new(Metadata::from_device(&device, index)?);
        Ok(Self {
            session: Arc::new(Mutex::new(Some(device))),
            metadata,
        })
    }

    /// Cached context with IIO devices, channels, attributes, and scan layouts.
    pub fn context(&self) -> &IioContext {
        &self.metadata.context
    }

    /// Close the shared USB session for all clones; safe to repeat or retry.
    /// Drop all RX stream handles first, including stopped streams, or this returns Busy.
    /// The metadata snapshot remains readable after shutdown.
    pub fn shutdown(&self) -> Result<(), Error> {
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        if let Some(device) = session.as_mut() {
            device.shutdown().wait().map_err(map_error)?;
        }
        *session = None;
        Ok(())
    }
}

impl DeviceInfo for Pluto {
    fn driver(&self) -> Driver {
        Driver::Pluto
    }
    fn id(&self) -> Result<String, Error> {
        self.metadata.id()
    }
    fn info(&self) -> Result<Args, Error> {
        Ok(self.metadata.args.clone())
    }
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        Ok(usize::from(direction == Direction::Rx))
    }
    fn full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

crate::impl_dyn_device_backend!(Pluto => [rx, antenna, agc, gain, frequency, sample_rate, bandwidth, dc_offset]);
crate::registry::impl_typed_device_backend!(Pluto, Driver::Pluto);

impl Pluto {
    fn read_setting(
        &self,
        direction: Direction,
        channel: usize,
        attr: RxAttribute,
        available: bool,
    ) -> Result<String, Error> {
        check_channel(direction, channel)?;
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .read_rx_attribute(attr, available)
            .wait()
            .map_err(map_error)
    }
    fn setting_range(
        &self,
        direction: Direction,
        channel: usize,
        attr: RxAttribute,
    ) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        range(
            session
                .as_mut()
                .ok_or(Error::DeviceDisconnected)?
                .rx_range(attr)
                .wait()
                .map_err(map_error)?,
        )
    }
    fn write_setting(
        &self,
        direction: Direction,
        channel: usize,
        attr: RxAttribute,
        value: &str,
    ) -> Result<(), Error> {
        check_channel(direction, channel)?;
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .set_rx_attribute(attr, value)
            .wait()
            .map_err(map_error)
    }
}

impl crate::AntennaControl for Pluto {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        Ok(self
            .read_setting(direction, channel, RxAttribute::Port, true)?
            .split_whitespace()
            .map(str::to_owned)
            .collect())
    }
    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.read_setting(direction, channel, RxAttribute::Port, false)
    }
    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        self.write_setting(direction, channel, RxAttribute::Port, name)
    }
}

impl crate::AgcControl for Pluto {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_channel(direction, channel)?;
        Ok(true)
    }
    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self.read_setting(direction, channel, RxAttribute::GainMode, false)? != "manual")
    }
    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        self.write_setting(
            direction,
            channel,
            RxAttribute::GainMode,
            if enabled { "slow_attack" } else { "manual" },
        )
    }
}

impl crate::DcOffsetControl for Pluto {
    fn dc_offset_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_channel(direction, channel)?;
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        Ok(session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .dc_offset_available())
    }
    fn dc_offset_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_channel(direction, channel)?;
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .dc_offset_enabled()
            .wait()
            .map_err(map_error)
    }
    fn set_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        check_channel(direction, channel)?;
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .set_dc_offset_enabled(enabled)
            .wait()
            .map_err(map_error)
    }
}

impl crate::GainControl for Pluto {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(vec!["RX".into()])
    }
    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.write_setting(direction, channel, RxAttribute::Gain, &gain.to_string())
    }
    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        numeric(self.read_setting(direction, channel, RxAttribute::Gain, false)?).map(Some)
    }
    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.setting_range(direction, channel, RxAttribute::Gain)
    }
    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        check_channel(direction, channel)?;
        named(name, "RX")?;
        self.write_setting(direction, channel, RxAttribute::Gain, &gain.to_string())
    }
    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        check_channel(direction, channel)?;
        named(name, "RX")?;
        numeric(self.read_setting(direction, channel, RxAttribute::Gain, false)?).map(Some)
    }
    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        named(name, "RX")?;
        self.setting_range(direction, channel, RxAttribute::Gain)
    }
}

impl crate::FrequencyControl for Pluto {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.setting_range(direction, channel, RxAttribute::Frequency)
    }
    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        numeric(self.read_setting(direction, channel, RxAttribute::Frequency, false)?)
    }
    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        check_channel(direction, channel)?;
        if !args.map().is_empty() {
            return Err(Error::invalid_argument(
                "args",
                "Pluto tuning arguments are unsupported",
            ));
        }
        self.write_setting(
            direction,
            channel,
            RxAttribute::Frequency,
            &frequency.to_string(),
        )
    }
    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(vec!["RF".into()])
    }
    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        named(name, "RF")?;
        self.setting_range(direction, channel, RxAttribute::Frequency)
    }
    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        check_channel(direction, channel)?;
        named(name, "RF")?;
        numeric(self.read_setting(direction, channel, RxAttribute::Frequency, false)?)
    }
    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        check_channel(direction, channel)?;
        named(name, "RF")?;
        self.write_setting(
            direction,
            channel,
            RxAttribute::Frequency,
            &frequency.to_string(),
        )
    }
}

impl crate::SampleRateControl for Pluto {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        numeric(self.read_setting(direction, channel, RxAttribute::SampleRate, false)?)
    }
    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.write_setting(
            direction,
            channel,
            RxAttribute::SampleRate,
            &rate.to_string(),
        )
    }
    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.setting_range(direction, channel, RxAttribute::SampleRate)
    }
}

impl crate::BandwidthControl for Pluto {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        numeric(self.read_setting(direction, channel, RxAttribute::Bandwidth, false)?)
    }
    fn set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bandwidth: f64,
    ) -> Result<(), Error> {
        self.write_setting(
            direction,
            channel,
            RxAttribute::Bandwidth,
            &bandwidth.to_string(),
        )
    }
    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.setting_range(direction, channel, RxAttribute::Bandwidth)
    }
}

/// A single-channel Pluto RX stream owning its independent USB pipe.
pub struct PlutoRxStreamer {
    inner: plutosdr::RxStream,
}
impl crate::RxDevice for Pluto {
    type RxStreamer = PlutoRxStreamer;
    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error> {
        let samples = buffer_samples(channels, &args)?;
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        let inner = session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .rx_stream_with_buffer(samples)
            .map_err(map_error)?;
        Ok(PlutoRxStreamer { inner })
    }
}
impl crate::RxStreamer for PlutoRxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(self.inner.mtu())
    }
    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        self.inner.start().wait().map_err(map_error)
    }
    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        self.inner.stop().wait().map_err(map_error)
    }
    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        let timeout = if timeout_us < 0 {
            None
        } else {
            Some(std::time::Duration::from_micros(timeout_us as u64))
        };
        self.inner
            .read(buffers[0], timeout)
            .wait()
            .map_err(map_error)
    }
}
