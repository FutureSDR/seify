use crate::async_compat::Shared;
use futures::lock::Mutex;

use num_complex::Complex32;
use uhd_rs::{Device as UhdDevice, RxGain, RxStream};

use super::common::*;
use crate::{
    Args, AsyncAgcControl, AsyncAntennaControl, AsyncDeviceInfo, AsyncFrequencyControl,
    AsyncGainControl, AsyncRxDevice, AsyncSampleRateControl, Capability, Direction, Driver, Error,
    Range,
};

/// Asynchronous native Rust USRP B2xx RX device. Clones share hardware and configuration.
#[derive(Clone)]
pub struct AsyncUhd {
    state: Shared<Mutex<State>>,
    metadata: Args,
}

/// Exclusively claimed B2xx receive stream, retaining ownership of the hardware.
///
/// Dropping the stream lets `uhd-rs` clean up its USB queue and hardware claim.
pub struct AsyncUhdRxStreamer {
    stream: RxStream,
    active: bool,
}

impl AsyncUhd {
    /// Discover B2xx devices. `index` takes precedence over a string `serial`.
    /// Supports channel-zero RX on B200, B210, B200mini, and B205mini.
    pub async fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        device_selector(args)?;
        probe_args(args, UhdDevice::list().await?)
    }

    /// Open a B2xx without starting RX. Firmware and FPGA images are embedded.
    pub async fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        // Validate selectors before performing discovery.
        device_selector(&args)?;
        let (index, descriptor) = select(&args, UhdDevice::list().await?, |d| {
            d.serial_number.as_deref()
        })?
        .into_iter()
        .next()
        .ok_or(Error::DeviceNotFound)?;
        let mut metadata = device_args(index, &descriptor);
        let mut device = UhdDevice::builder().descriptor(descriptor).open().await?;
        update_identity(&mut metadata, &device.identity().await?);
        // The driver has no configuration getters. Retune once to obtain the
        // actual quantized frequency instead of caching the requested value.
        let frequency = device.set_center_frequency(DEFAULT_FREQUENCY).await?;
        let rate = device.set_sample_rate(DEFAULT_RATE).await?;
        Ok(Self {
            state: Shared::new(Mutex::new(State {
                device,
                frequency: Some(frequency),
                rate: Some(rate),
                gain: Some(RxGain::Manual(DEFAULT_GAIN)),
                manual_gain: DEFAULT_GAIN,
            })),
            metadata,
        })
    }

    /// Release the device explicitly after closing all stream claims.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let mut state = self.state.lock().await;
        let previous = (state.frequency.take(), state.rate.take(), state.gain.take());
        match state.device.shutdown().await {
            // Busy is a nonterminal rejection: an existing stream still owns
            // the device, so its configuration remains valid.
            Err(uhd_rs::Error::Busy) => {
                (state.frequency, state.rate, state.gain) = previous;
                Err(Error::Busy)
            }
            result => result.map_err(Error::from),
        }
    }
}

impl AsyncDeviceInfo for AsyncUhd {
    fn driver(&self) -> Driver {
        Driver::Uhd
    }
    async fn async_id(&self) -> Result<String, Error> {
        if let Some(serial) = optional_arg::<String>(&self.metadata, "serial")? {
            Ok(serial)
        } else {
            self.metadata.get("index")
        }
    }
    async fn async_info(&self) -> Result<Args, Error> {
        Ok(self.metadata.clone())
    }
    async fn async_num_channels(&self, direction: Direction) -> Result<usize, Error> {
        Ok(usize::from(direction == Direction::Rx))
    }
    async fn async_full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

crate::impl_dyn_async_device_backend!(AsyncUhd => [rx, antenna, agc, gain, frequency, sample_rate]);

impl AsyncRxDevice for AsyncUhd {
    type RxStreamer = AsyncUhdRxStreamer;
    async fn async_rx_streamer(
        &self,
        channels: &[usize],
        _args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "channels",
                "UHD RX requires exactly channel 0",
            ));
        }
        let stream = self.state.lock().await.device.rx_stream()?;
        Ok(AsyncUhdRxStreamer {
            stream,
            active: false,
        })
    }
}

impl AsyncAntennaControl for AsyncUhd {
    async fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(vec!["RX2".into()])
    }
    async fn async_antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        check_rx(direction, channel)?;
        Ok("RX2".into())
    }
    async fn async_set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        check_name(name, "RX2")
    }
}

impl AsyncAgcControl for AsyncUhd {
    async fn async_agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(true)
    }
    async fn async_agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(matches!(
            known(self.state.lock().await.gain)?,
            RxGain::Automatic
        ))
    }
    async fn async_set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let mut state = self.state.lock().await;
        let gain = if enabled {
            RxGain::Automatic
        } else {
            RxGain::Manual(state.manual_gain)
        };
        state.gain = None;
        state.device.set_gain(gain).await?;
        state.gain = Some(gain);
        Ok(())
    }
}

impl AsyncGainControl for AsyncUhd {
    async fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(vec!["PGA".into()])
    }
    async fn async_gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(gain_range())
    }
    async fn async_gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        check_rx(direction, channel)?;
        Ok(match known(self.state.lock().await.gain)? {
            RxGain::Automatic => None,
            RxGain::Manual(gain) => Some(gain),
        })
    }
    async fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        if !gain.is_finite() || !(0.0..=76.0).contains(&gain) {
            return Err(Error::out_of_range("gain", gain_range(), gain));
        }
        let gain = gain.round();
        let mut state = self.state.lock().await;
        state.gain = None;
        state.device.set_gain(RxGain::Manual(gain)).await?;
        state.gain = Some(RxGain::Manual(gain));
        state.manual_gain = gain;
        Ok(())
    }
    async fn async_gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        check_name(name, "PGA")?;
        self.async_gain_range(direction, channel).await
    }
    async fn async_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        check_rx(direction, channel)?;
        check_name(name, "PGA")?;
        self.async_gain(direction, channel).await
    }
    async fn async_set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        check_name(name, "PGA")?;
        self.async_set_gain(direction, channel, gain).await
    }
}

impl AsyncFrequencyControl for AsyncUhd {
    async fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(frequency_range())
    }
    async fn async_frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        known(self.state.lock().await.frequency)
    }
    async fn async_set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let request = tune_request(frequency, &args)?;
        let mut state = self.state.lock().await;
        state.frequency = None;
        let result = state.device.tune(request).await?;
        state.frequency = Some(result.actual_center_frequency_hz);
        Ok(())
    }
    async fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(vec!["TUNER".into()])
    }
    async fn async_component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        check_name(name, "TUNER")?;
        self.async_frequency_range(direction, channel).await
    }
    async fn async_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        check_name(name, "TUNER")?;
        self.async_frequency(direction, channel).await
    }
    async fn async_set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        check_name(name, "TUNER")?;
        self.async_set_frequency(direction, channel, frequency, Args::new())
            .await
    }
}

impl AsyncSampleRateControl for AsyncUhd {
    async fn async_sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        known(self.state.lock().await.rate)
    }
    async fn async_set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        validate_rate(rate)?;
        let mut state = self.state.lock().await;
        state.rate = None;
        let actual = state.device.set_sample_rate(rate).await?;
        state.rate = Some(actual);
        Ok(())
    }
    async fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(sample_rate_range())
    }
}

impl AsyncUhdRxStreamer {
    /// Close the stream and release its exclusive claim, observing cleanup errors.
    pub async fn close(self) -> Result<uhd_rs::StreamingStats, Error> {
        Ok(self.stream.close().await?)
    }
}

impl crate::AsyncRxStreamer for AsyncUhdRxStreamer {
    async fn mtu(&self) -> Result<usize, Error> {
        Ok(RX_MTU)
    }
    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        self.stream.start().await?;
        self.active = true;
        Ok(())
    }
    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        self.active = false;
        self.stream.stop().await?;
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
        let count = buffers[0].len().min(RX_MTU);
        Ok(self
            .stream
            .read(&mut buffers[0][..count], timeout(timeout_us))
            .await?)
    }
}

impl crate::dev::AsyncTypedDeviceBackend for AsyncUhd {
    fn driver() -> Driver {
        Driver::Uhd
    }
    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(args: &Args) -> Result<Vec<crate::dev::WebUsbDeviceFilter>, Error> {
        webusb_filters(args)
    }
    async fn async_probe(args: &Args) -> Result<Vec<Args>, Error> {
        Self::probe(args).await
    }
    async fn async_open(args: &Args) -> Result<Self, Error> {
        Self::open(args).await
    }
}
