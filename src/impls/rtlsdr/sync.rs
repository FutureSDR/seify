use std::sync::{Arc, Mutex};
use std::time::Duration;

use num_complex::Complex32;
use rtlsdr_nusb::{Device as RtlSdrDevice, GainConfig, MaybeFuture, RxStream};

use super::common::*;
use crate::Direction::*;
use crate::{
    AgcControl, AntennaControl, Args, Capability, DeviceInfo, Direction, Driver, Error,
    FrequencyControl, GainControl, Range, RxDevice, SampleRateControl,
};

/// RTL-SDR device backend.
#[derive(Clone)]
pub struct RtlSdr {
    device: Arc<Mutex<RtlSdrDevice>>,
    metadata: Args,
    inner: Arc<ReceiverContext>,
}
/// Exclusively claimed RTL-SDR receive streamer.
///
/// The streamer shares ownership of the hardware with the device, which remains
/// available for control operations while reception is active. The driver
/// performs final receiver and queue cleanup when the stream is dropped.
/// Reads may return fewer samples than the supplied buffer can hold.
pub struct RxStreamer {
    stream: RxStream,
    active: bool,
    cleanup_required: bool,
}

trait RtlSdrDeviceControl {
    fn set_frequency_hz_sync(&mut self, frequency_hz: u64) -> Result<(), Error>;
    fn set_sample_rate_hz_sync(&mut self, sample_rate_hz: u32) -> Result<(), Error>;
    fn set_gain_sync(&mut self, gain: GainConfig) -> Result<(), Error>;
}

impl RtlSdrDeviceControl for RtlSdrDevice {
    fn set_frequency_hz_sync(&mut self, frequency_hz: u64) -> Result<(), Error> {
        RtlSdrDevice::set_frequency_hz(self, frequency_hz)
            .wait()
            .map_err(map_rtlsdr_error)
    }

    fn set_sample_rate_hz_sync(&mut self, sample_rate_hz: u32) -> Result<(), Error> {
        RtlSdrDevice::set_sample_rate_hz(self, sample_rate_hz)
            .wait()
            .map_err(map_rtlsdr_error)
    }

    fn set_gain_sync(&mut self, gain: GainConfig) -> Result<(), Error> {
        RtlSdrDevice::set_gain(self, gain)
            .wait()
            .map_err(map_rtlsdr_error)
    }
}

impl RtlSdr {
    /// Return descriptors for detected RTL-SDR devices.
    ///
    /// USB serials are preserved as strings. An `index` selector takes
    /// precedence over `serial`, as it does when opening a device.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let devices = RtlSdrDevice::list().wait().map_err(map_rtlsdr_error)?;
        probe_args(args, devices)
    }

    /// Open an RTL-SDR device from arguments.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let dev = selected_builder(&args)?
            .open()
            .wait()
            .map_err(map_rtlsdr_error)?;
        let receiver_context = ReceiverContext::from_device_info(dev.info());
        let metadata = device_args(dev.info());

        Ok(Self {
            device: Arc::new(Mutex::new(dev)),
            metadata,
            inner: Arc::new(receiver_context),
        })
    }

    fn with_device<T>(
        &self,
        operation: impl FnOnce(&mut RtlSdrDevice) -> Result<T, Error>,
    ) -> Result<T, Error> {
        operation(&mut self.device.lock().unwrap())
    }
}

impl RtlSdr {
    fn driver(&self) -> Driver {
        Driver::RtlSdr
    }

    fn id(&self) -> Result<String, Error> {
        self.metadata.get::<String>("index")
    }

    fn info(&self) -> Result<Args, Error> {
        Ok(self.metadata.clone())
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        match direction {
            Rx => Ok(1),
            Tx => Ok(0),
        }
    }

    fn full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(self.inner.antennas())
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        check_rx(direction, channel)?;
        Ok("RX".to_owned())
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        check_rx(direction, channel)?;
        if name != "RX" {
            return Err(Error::invalid_argument(
                "antenna",
                "RTL-SDR exposes only RX",
            ));
        }
        Ok(())
    }

    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(true)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        self.with_device(|device| {
            let gain = agc_gain_config(device.config(), agc)?;
            device.set_gain_sync(gain)
        })
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        self.with_device(|device| self.inner.agc_enabled(device.config()))
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(self
            .inner
            .gains
            .iter()
            .map(|gain| gain.name.to_string())
            .collect())
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let range = overall_gain_range();
        if !range.contains(gain) {
            return Err(Error::out_of_range("gain", range, gain));
        }

        self.with_device(|device| device.set_gain_sync(overall_gain_config(gain)))
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        check_rx(direction, channel)?;
        self.with_device(|device| self.inner.overall_gain(device.config()))
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(overall_gain_range())
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let gain_type = gain_type(name).ok_or(Error::invalid_argument(
            "rtlsdr",
            "invalid RTL-SDR argument",
        ))?;
        let range = self.gain_element_range(direction, channel, name)?;
        if !range.contains(gain) {
            return Err(Error::out_of_range("gain", range, gain));
        }

        self.with_device(|device| {
            let gain = gain_type.update(device.config(), gain)?;
            device.set_gain_sync(gain)
        })
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        check_rx(direction, channel)?;
        let gain_type = gain_type(name).ok_or(Error::invalid_argument(
            "rtlsdr",
            "invalid RTL-SDR argument",
        ))?;
        self.with_device(|device| self.inner.gain_value(device.config(), gain_type))
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        let gain_type = gain_type(name).ok_or(Error::invalid_argument(
            "rtlsdr",
            "invalid RTL-SDR argument",
        ))?;
        self.inner
            .gain_range(gain_type)
            .ok_or(Error::invalid_argument(
                "rtlsdr",
                "invalid RTL-SDR argument",
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
        check_rx(direction, channel)?;
        Ok(vec!["TUNER".to_string()])
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        if name == "TUNER" {
            self.inner.frequency_range()
        } else {
            Err(Error::invalid_argument(
                "rtlsdr",
                "invalid RTL-SDR argument",
            ))
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        if name != "TUNER" {
            return Err(Error::invalid_argument(
                "rtlsdr",
                "invalid RTL-SDR argument",
            ));
        }
        self.with_device(|device| Ok(device.actual_frequency_hz() as f64))
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
        check_rx(direction, channel)?;
        self.with_device(|device| Ok(device.actual_sample_rate_hz() as f64))
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
        check_rx(direction, channel)?;
        Ok(sample_rate_range())
    }
}

impl DeviceInfo for RtlSdr {
    fn driver(&self) -> Driver {
        RtlSdr::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        RtlSdr::id(self)
    }

    fn info(&self) -> Result<Args, Error> {
        RtlSdr::info(self)
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        RtlSdr::num_channels(self, direction)
    }

    fn full_duplex(&self) -> Result<bool, Error> {
        RtlSdr::full_duplex(self)
    }
}

crate::impl_dyn_device_backend!(
    RtlSdr => [rx, antenna, agc, gain, frequency, sample_rate]
);
crate::registry::impl_typed_device_backend!(RtlSdr, Driver::RtlSdr);

impl RxDevice for RtlSdr {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "rtlsdr",
                "invalid RTL-SDR argument",
            ));
        }
        let stream = self
            .device
            .lock()
            .unwrap()
            .rx_stream()
            .map_err(map_rtlsdr_error)?;
        Ok(RxStreamer::new(stream))
    }
}

impl AntennaControl for RtlSdr {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        RtlSdr::antennas(self, direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        RtlSdr::antenna(self, direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        RtlSdr::set_antenna(self, direction, channel, name)
    }
}

impl AgcControl for RtlSdr {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        RtlSdr::agc_available(self, direction, channel)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        RtlSdr::set_agc_enabled(self, direction, channel, agc)
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        RtlSdr::agc_enabled(self, direction, channel)
    }
}

impl GainControl for RtlSdr {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        RtlSdr::gain_elements(self, direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        RtlSdr::set_gain(self, direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        RtlSdr::gain(self, direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        RtlSdr::gain_range(self, direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        RtlSdr::set_gain_element(self, direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        RtlSdr::gain_element(self, direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        RtlSdr::gain_element_range(self, direction, channel, name)
    }
}

impl FrequencyControl for RtlSdr {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        RtlSdr::frequency_range(self, direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        RtlSdr::frequency(self, direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        RtlSdr::set_frequency(self, direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        RtlSdr::frequency_components(self, direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        RtlSdr::component_frequency_range(self, direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        RtlSdr::component_frequency(self, direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        RtlSdr::set_component_frequency(self, direction, channel, name, frequency)
    }
}

impl SampleRateControl for RtlSdr {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        RtlSdr::sample_rate(self, direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        RtlSdr::set_sample_rate(self, direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        RtlSdr::get_sample_rate_range(self, direction, channel)
    }
}

impl RxStreamer {
    fn new(stream: RxStream) -> Self {
        Self {
            stream,
            active: false,
            cleanup_required: false,
        }
    }

    fn stop(&mut self) -> Result<(), Error> {
        if self.cleanup_required {
            self.stream.stop().wait().map_err(map_rtlsdr_error)?;
            self.cleanup_required = false;
        }
        self.active = false;
        Ok(())
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
        if self.active {
            return Ok(());
        }
        self.cleanup_required = true;
        self.stream.start().wait().map_err(map_rtlsdr_error)?;
        self.active = true;
        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        self.stop()
    }

    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        if buffers[0].is_empty() {
            return Ok(0);
        }

        let out = &mut buffers[0];
        let read_len = out.len().min(F32_RX_MTU);
        let timeout = if timeout_us < 0 {
            None
        } else {
            Some(Duration::from_micros(timeout_us as u64))
        };
        self.stream
            .read(&mut out[..read_len], timeout)
            .wait()
            .map_err(map_rtlsdr_error)
    }
}
