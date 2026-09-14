use super::{
    common::{
        buffer_samples, check_channel, map_error, named, numeric, probe_args, range, Metadata,
        Selector,
    },
    IioContext,
};
use crate::{
    async_compat::Shared, dev::AsyncTypedDeviceBackend, Args, AsyncDeviceInfo, Direction, Driver,
    Error,
};
use crate::{Capability, Range};
use futures::lock::Mutex;
use num_complex::Complex32;
use plutosdr::Device as PlutoDevice;
use plutosdr::RxAttribute;

/// Asynchronous PlutoSDR RX backend, including browser WebUSB.
/// Clones share one USB session and its shutdown state.
#[derive(Clone)]
pub struct AsyncPluto {
    session: Shared<Mutex<Option<PlutoDevice>>>,
    metadata: Shared<Metadata>,
}

impl AsyncPluto {
    /// Probe standard Pluto USB devices already authorized on browser targets.
    pub async fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let selector = Selector::from_args(args)?;
        let devices = PlutoDevice::list().await.map_err(map_error)?;
        Ok(selector
            .select(devices)
            .map(|(i, d)| probe_args(i, &d))
            .collect())
    }

    /// Open an IIO session and retrieve its XML context without RF configuration.
    /// On WebUSB, obtain permission through AsyncRegistry before opening.
    pub async fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = Selector::from_args(&args)?;
        let devices = PlutoDevice::list().await.map_err(map_error)?;
        let (index, descriptor) = selector
            .select(devices)
            .next()
            .ok_or(Error::DeviceNotFound)?;
        let device = PlutoDevice::builder()
            .descriptor(descriptor)
            .open()
            .await
            .map_err(map_error)?;
        let metadata = Shared::new(Metadata::from_device(&device, index)?);
        Ok(Self {
            session: Shared::new(Mutex::new(Some(device))),
            metadata,
        })
    }

    /// Cached context; inspecting it performs no USB I/O.
    pub fn context(&self) -> &IioContext {
        &self.metadata.context
    }

    /// Close the session shared by all clones; repeat/retry safely.
    /// Drop all RX stream handles first, including stopped streams, or this returns Busy.
    /// The metadata snapshot remains readable after shutdown.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let mut session = self.session.lock().await;
        if let Some(device) = session.as_mut() {
            device.shutdown().await.map_err(map_error)?;
        }
        *session = None;
        Ok(())
    }
}

impl AsyncDeviceInfo for AsyncPluto {
    fn driver(&self) -> Driver {
        Driver::Pluto
    }
    async fn async_id(&self) -> Result<String, Error> {
        self.metadata.id()
    }
    async fn async_info(&self) -> Result<Args, Error> {
        Ok(self.metadata.args.clone())
    }
    async fn async_num_channels(&self, direction: Direction) -> Result<usize, Error> {
        Ok(usize::from(direction == Direction::Rx))
    }
    async fn async_full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

crate::impl_dyn_async_device_backend!(AsyncPluto => [rx, antenna, agc, gain, frequency, sample_rate, bandwidth]);

impl AsyncTypedDeviceBackend for AsyncPluto {
    fn driver() -> Driver {
        Driver::Pluto
    }
    async fn async_probe(args: &Args) -> Result<Vec<Args>, Error> {
        Self::probe(args).await
    }
    async fn async_open(args: &Args) -> Result<Self, Error> {
        Self::open(args.clone()).await
    }

    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(args: &Args) -> Result<Vec<crate::dev::WebUsbDeviceFilter>, Error> {
        use plutosdr::usb::discovery::{PLUTO_PID, PLUTO_VID};
        let mut filter =
            crate::dev::WebUsbDeviceFilter::new().with_vendor_product(PLUTO_VID, PLUTO_PID);
        if let Selector::Serial(serial) = Selector::from_args(args)? {
            filter = filter.with_serial_number(serial);
        }
        Ok(vec![filter])
    }
}

impl AsyncPluto {
    async fn read_setting(
        &self,
        direction: Direction,
        channel: usize,
        attr: RxAttribute,
        available: bool,
    ) -> Result<String, Error> {
        check_channel(direction, channel)?;
        let mut session = self.session.lock().await;
        session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .read_rx_attribute(attr, available)
            .await
            .map_err(map_error)
    }
    async fn setting_range(
        &self,
        direction: Direction,
        channel: usize,
        attr: RxAttribute,
    ) -> Result<Range, Error> {
        let value = self.read_setting(direction, channel, attr, true).await?;
        range(plutosdr::ValueRange::parse(&value).map_err(map_error)?)
    }
    async fn write_setting(
        &self,
        direction: Direction,
        channel: usize,
        attr: RxAttribute,
        value: &str,
    ) -> Result<(), Error> {
        check_channel(direction, channel)?;
        let mut session = self.session.lock().await;
        session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .set_rx_attribute(attr, value)
            .await
            .map_err(map_error)
    }
}

impl crate::AsyncAntennaControl for AsyncPluto {
    async fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        Ok(self
            .read_setting(direction, channel, RxAttribute::Port, true)
            .await?
            .split_whitespace()
            .map(str::to_owned)
            .collect())
    }
    async fn async_antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.read_setting(direction, channel, RxAttribute::Port, false)
            .await
    }
    async fn async_set_antenna<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> Result<(), Error> {
        self.write_setting(direction, channel, RxAttribute::Port, name)
            .await
    }
}

impl crate::AsyncAgcControl for AsyncPluto {
    async fn async_agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<bool, Error> {
        check_channel(direction, channel)?;
        Ok(true)
    }
    async fn async_agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self
            .read_setting(direction, channel, RxAttribute::GainMode, false)
            .await?
            != "manual")
    }
    async fn async_set_agc_enabled(
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
        .await
    }
}

impl crate::AsyncGainControl for AsyncPluto {
    async fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(vec!["RX".into()])
    }
    async fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        self.write_setting(direction, channel, RxAttribute::Gain, &gain.to_string())
            .await
    }
    async fn async_gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        numeric(
            self.read_setting(direction, channel, RxAttribute::Gain, false)
                .await?,
        )
        .map(Some)
    }
    async fn async_gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.setting_range(direction, channel, RxAttribute::Gain)
            .await
    }
    async fn async_set_gain_element<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
        gain: f64,
    ) -> Result<(), Error> {
        check_channel(direction, channel)?;
        named(name, "RX")?;
        self.write_setting(direction, channel, RxAttribute::Gain, &gain.to_string())
            .await
    }
    async fn async_gain_element<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> Result<Option<f64>, Error> {
        check_channel(direction, channel)?;
        named(name, "RX")?;
        numeric(
            self.read_setting(direction, channel, RxAttribute::Gain, false)
                .await?,
        )
        .map(Some)
    }
    async fn async_gain_element_range<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        named(name, "RX")?;
        self.setting_range(direction, channel, RxAttribute::Gain)
            .await
    }
}

impl crate::AsyncFrequencyControl for AsyncPluto {
    async fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        self.setting_range(direction, channel, RxAttribute::Frequency)
            .await
    }
    async fn async_frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        numeric(
            self.read_setting(direction, channel, RxAttribute::Frequency, false)
                .await?,
        )
    }
    async fn async_set_frequency(
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
        .await
    }
    async fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_channel(direction, channel)?;
        Ok(vec!["RF".into()])
    }
    async fn async_component_frequency_range<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> Result<Range, Error> {
        check_channel(direction, channel)?;
        named(name, "RF")?;
        self.setting_range(direction, channel, RxAttribute::Frequency)
            .await
    }
    async fn async_component_frequency<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> Result<f64, Error> {
        check_channel(direction, channel)?;
        named(name, "RF")?;
        numeric(
            self.read_setting(direction, channel, RxAttribute::Frequency, false)
                .await?,
        )
    }
    async fn async_set_component_frequency<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
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
        .await
    }
}

impl crate::AsyncSampleRateControl for AsyncPluto {
    async fn async_sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        numeric(
            self.read_setting(direction, channel, RxAttribute::SampleRate, false)
                .await?,
        )
    }
    async fn async_set_sample_rate(
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
        .await
    }
    async fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        self.setting_range(direction, channel, RxAttribute::SampleRate)
            .await
    }
}

impl crate::AsyncBandwidthControl for AsyncPluto {
    async fn async_bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        numeric(
            self.read_setting(direction, channel, RxAttribute::Bandwidth, false)
                .await?,
        )
    }
    async fn async_set_bandwidth(
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
        .await
    }
    async fn async_get_bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        self.setting_range(direction, channel, RxAttribute::Bandwidth)
            .await
    }
}

/// A single-channel Pluto RX stream owning its independent USB pipe.
pub struct AsyncPlutoRxStreamer {
    inner: plutosdr::RxStream,
}
impl crate::AsyncRxDevice for AsyncPluto {
    type RxStreamer = AsyncPlutoRxStreamer;
    async fn async_rx_streamer(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        let samples = buffer_samples(channels, &args)?;
        let mut session = self.session.lock().await;
        let inner = session
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .rx_stream_with_buffer(samples)
            .map_err(map_error)?;
        Ok(AsyncPlutoRxStreamer { inner })
    }
}
impl crate::AsyncRxStreamer for AsyncPlutoRxStreamer {
    async fn mtu(&self) -> Result<usize, Error> {
        Ok(self.inner.mtu())
    }
    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        self.inner.start().await.map_err(map_error)
    }
    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        self.inner.stop().await.map_err(map_error)
    }
    async fn read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> Result<usize, Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        let timeout = if timeout_us < 0 {
            None
        } else {
            Some(std::time::Duration::from_micros(timeout_us as u64))
        };
        self.inner
            .read(buffers[0], timeout)
            .await
            .map_err(map_error)
    }
}
