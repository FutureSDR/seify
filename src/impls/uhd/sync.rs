use std::sync::{Arc, Mutex};

use num_complex::Complex32;
use uhd_rs::{Device as UhdDevice, MaybeFuture, RxGain, RxStream};

use super::common::*;
use crate::{
    AgcControl, AntennaControl, Args, Capability, DeviceInfo, Direction, Driver, Error,
    FrequencyControl, GainControl, Range, RxDevice, SampleRateControl,
};

/// Native Rust USRP B2xx RX device. Clones share hardware and configuration.
#[derive(Clone)]
pub struct Uhd {
    state: Arc<Mutex<State>>,
    metadata: Args,
}

/// Exclusively claimed B2xx receive stream, retaining ownership of the hardware.
///
/// Dropping the stream lets `uhd-rs` clean up its USB queue and hardware claim.
pub struct RxStreamer {
    stream: RxStream,
    active: bool,
}

impl Uhd {
    /// Discover B2xx devices. `index` takes precedence over a string `serial`.
    /// Supports channel-zero RX on B200, B210, B200mini, and B205mini.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        device_selector(args)?;
        probe_args(args, UhdDevice::list().wait()?)
    }

    /// Open a B2xx without starting RX. Firmware and FPGA images are embedded.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        // Validate selectors before performing discovery.
        device_selector(&args)?;
        let (index, descriptor) = select(&args, UhdDevice::list().wait()?, |d| {
            d.serial_number.as_deref()
        })?
        .into_iter()
        .next()
        .ok_or(Error::DeviceNotFound)?;
        let mut metadata = device_args(index, &descriptor);
        let mut device = UhdDevice::builder().descriptor(descriptor).open().wait()?;
        update_identity(&mut metadata, &device.identity().wait()?);
        // The driver has no configuration getters. Retune once to obtain the
        // actual quantized frequency instead of caching the requested value.
        let frequency = device.set_center_frequency(DEFAULT_FREQUENCY).wait()?;
        let rate = device.set_sample_rate(DEFAULT_RATE).wait()?;
        Ok(Self {
            state: Arc::new(Mutex::new(State {
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
    pub fn shutdown(&self) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        let previous = (state.frequency.take(), state.rate.take(), state.gain.take());
        match state.device.shutdown().wait() {
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

impl DeviceInfo for Uhd {
    fn driver(&self) -> Driver {
        Driver::Uhd
    }
    fn id(&self) -> Result<String, Error> {
        if let Some(serial) = optional_arg::<String>(&self.metadata, "serial")? {
            Ok(serial)
        } else {
            self.metadata.get("index")
        }
    }
    fn info(&self) -> Result<Args, Error> {
        Ok(self.metadata.clone())
    }
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        Ok(usize::from(direction == Direction::Rx))
    }
    fn full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

crate::impl_dyn_device_backend!(Uhd => [rx, antenna, agc, gain, frequency, sample_rate]);
crate::registry::impl_typed_device_backend!(Uhd, Driver::Uhd);

impl RxDevice for Uhd {
    type RxStreamer = RxStreamer;
    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "channels",
                "UHD RX requires exactly channel 0",
            ));
        }
        let stream = self.state.lock().unwrap().device.rx_stream()?;
        Ok(RxStreamer {
            stream,
            active: false,
        })
    }
}

impl AntennaControl for Uhd {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(vec!["RX2".into()])
    }
    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        check_rx(direction, channel)?;
        Ok("RX2".into())
    }
    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        check_rx(direction, channel)?;
        check_name(name, "RX2")
    }
}

impl AgcControl for Uhd {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(true)
    }
    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(matches!(
            known(self.state.lock().unwrap().gain)?,
            RxGain::Automatic
        ))
    }
    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let mut state = self.state.lock().unwrap();
        let gain = if enabled {
            RxGain::Automatic
        } else {
            RxGain::Manual(state.manual_gain)
        };
        state.gain = None;
        state.device.set_gain(gain).wait()?;
        state.gain = Some(gain);
        Ok(())
    }
}

impl GainControl for Uhd {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(vec!["PGA".into()])
    }
    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(gain_range())
    }
    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        check_rx(direction, channel)?;
        Ok(match known(self.state.lock().unwrap().gain)? {
            RxGain::Automatic => None,
            RxGain::Manual(gain) => Some(gain),
        })
    }
    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        check_rx(direction, channel)?;
        if !gain.is_finite() || !(0.0..=76.0).contains(&gain) {
            return Err(Error::out_of_range("gain", gain_range(), gain));
        }
        let gain = gain.round();
        let mut state = self.state.lock().unwrap();
        state.gain = None;
        state.device.set_gain(RxGain::Manual(gain)).wait()?;
        state.gain = Some(RxGain::Manual(gain));
        state.manual_gain = gain;
        Ok(())
    }
    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        check_name(name, "PGA")?;
        self.gain_range(direction, channel)
    }
    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        check_rx(direction, channel)?;
        check_name(name, "PGA")?;
        self.gain(direction, channel)
    }
    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        check_name(name, "PGA")?;
        self.set_gain(direction, channel, gain)
    }
}

impl FrequencyControl for Uhd {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(frequency_range())
    }
    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        known(self.state.lock().unwrap().frequency)
    }
    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let request = tune_request(frequency, &args)?;
        let mut state = self.state.lock().unwrap();
        state.frequency = None;
        let result = state.device.tune(request).wait()?;
        state.frequency = Some(result.actual_center_frequency_hz);
        Ok(())
    }
    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(vec!["TUNER".into()])
    }
    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        check_name(name, "TUNER")?;
        self.frequency_range(direction, channel)
    }
    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        check_name(name, "TUNER")?;
        self.frequency(direction, channel)
    }
    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        check_name(name, "TUNER")?;
        self.set_frequency(direction, channel, frequency, Args::new())
    }
}

impl SampleRateControl for Uhd {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        known(self.state.lock().unwrap().rate)
    }
    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        validate_rate(rate)?;
        let mut state = self.state.lock().unwrap();
        state.rate = None;
        let actual = state.device.set_sample_rate(rate).wait()?;
        state.rate = Some(actual);
        Ok(())
    }
    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(sample_rate_range())
    }
}

impl RxStreamer {
    /// Close the stream and release its exclusive claim, observing cleanup errors.
    pub fn close(self) -> Result<uhd_rs::StreamingStats, Error> {
        Ok(self.stream.close().wait()?)
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(RX_MTU)
    }
    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        self.stream.start().wait()?;
        self.active = true;
        Ok(())
    }
    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        self.active = false;
        self.stream.stop().wait()?;
        Ok(())
    }
    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        let count = buffers[0].len().min(RX_MTU);
        Ok(self
            .stream
            .read(&mut buffers[0][..count], timeout(timeout_us))
            .wait()?)
    }
}
