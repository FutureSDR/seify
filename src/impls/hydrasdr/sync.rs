use std::sync::{Arc, Mutex};
use std::time::Duration;

use hydrasdr_rs::{
    DecimationPolicy, Device as HydraSdrDevice, GainConfig, MaybeFuture, RfPort, RxStream,
};
use num_complex::Complex32;

use super::common::*;
use crate::Direction::*;
use crate::{
    AgcControl, AntennaControl, Args, Capability, DeviceInfo, Direction, Driver, Error,
    FrequencyControl, GainControl, Range, RxDevice, SampleRateControl,
};

/// HydraSDR RFOne device backend.
#[derive(Clone)]
pub struct HydraSdr {
    device: Arc<Mutex<HydraSdrDevice>>,
    serial: u64,
    inner: Arc<ReceiverContext>,
}
/// Exclusively claimed HydraSDR RFOne receive streamer.
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

trait HydraSdrDeviceControl {
    fn set_frequency_hz_sync(&mut self, frequency_hz: u64) -> Result<(), Error>;
    fn set_sample_rate_hz_sync(&mut self, sample_rate_hz: u32) -> Result<(), Error>;
    fn set_rf_port_sync(&mut self, port: RfPort) -> Result<(), Error>;
    fn set_gain_sync(&mut self, gain: GainConfig) -> Result<(), Error>;
}

impl HydraSdrDeviceControl for HydraSdrDevice {
    fn set_frequency_hz_sync(&mut self, frequency_hz: u64) -> Result<(), Error> {
        HydraSdrDevice::set_frequency_hz(self, frequency_hz)
            .wait()
            .map_err(map_hydrasdr_error)
    }

    fn set_sample_rate_hz_sync(&mut self, sample_rate_hz: u32) -> Result<(), Error> {
        HydraSdrDevice::set_sample_rate_hz(self, sample_rate_hz)
            .wait()
            .map_err(map_hydrasdr_error)
    }

    fn set_rf_port_sync(&mut self, port: RfPort) -> Result<(), Error> {
        HydraSdrDevice::set_rf_port(self, port)
            .wait()
            .map_err(map_hydrasdr_error)
    }

    fn set_gain_sync(&mut self, gain: GainConfig) -> Result<(), Error> {
        HydraSdrDevice::set_gain(self, gain)
            .wait()
            .map_err(map_hydrasdr_error)
    }
}

impl HydraSdr {
    /// Return descriptors for detected HydraSDR RFOne devices.
    ///
    /// Every descriptor contains a normalized serial. Zero represents a
    /// missing, invalid, or zero USB serial and may match any such device.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let devices = HydraSdrDevice::list().wait().map_err(map_hydrasdr_error)?;
        probe_args(args, devices)
    }

    /// Open a HydraSDR RFOne device from arguments.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = device_selector(&args)?;
        let (dev, serial) = open_selected_device(selector)?;
        let sample_rates = dev.sample_rates();
        let receiver_context = ReceiverContext::from_device_info(dev.info(), sample_rates);

        Ok(Self {
            device: Arc::new(Mutex::new(dev)),
            serial,
            inner: Arc::new(receiver_context),
        })
    }

    fn with_device<T>(
        &self,
        operation: impl FnOnce(&mut HydraSdrDevice) -> Result<T, Error>,
    ) -> Result<T, Error> {
        operation(&mut self.device.lock().unwrap())
    }
}

impl HydraSdr {
    fn driver(&self) -> Driver {
        Driver::HydraSdr
    }

    fn id(&self) -> Result<String, Error> {
        Ok(self.serial.to_string())
    }

    fn info(&self) -> Result<Args, Error> {
        let mut args = Args::default();
        args.set("driver", "hydrasdr");
        args.set("serial", self.id()?);
        Ok(args)
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
        self.with_device(|device| self.inner.antenna(device.config()))
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let port = self
            .inner
            .rf_port_for_antenna(name)
            .ok_or(Error::invalid_argument(
                "antenna",
                "antenna is not available on this HydraSDR device",
            ))?;
        self.with_device(|device| device.set_rf_port_sync(port))
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
            "hydrasdr",
            "invalid HydraSDR argument",
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
            "hydrasdr",
            "invalid HydraSDR argument",
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
            "hydrasdr",
            "invalid HydraSDR argument",
        ))?;
        self.inner
            .gain_range(gain_type)
            .ok_or(Error::invalid_argument(
                "hydrasdr",
                "invalid HydraSDR argument",
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
                "hydrasdr",
                "invalid HydraSDR argument",
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
                "hydrasdr",
                "invalid HydraSDR argument",
            ));
        }
        self.with_device(|device| self.inner.frequency(device.config()))
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
        self.with_device(|device| self.inner.sample_rate(device.config()))
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
        sample_rate_range(&self.inner.sample_rates)
    }
}

impl DeviceInfo for HydraSdr {
    fn driver(&self) -> Driver {
        HydraSdr::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        HydraSdr::id(self)
    }

    fn info(&self) -> Result<Args, Error> {
        HydraSdr::info(self)
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        HydraSdr::num_channels(self, direction)
    }

    fn full_duplex(&self) -> Result<bool, Error> {
        HydraSdr::full_duplex(self)
    }
}

crate::impl_dyn_device_backend!(
    HydraSdr => [rx, antenna, agc, gain, frequency, sample_rate]
);
crate::registry::impl_typed_device_backend!(HydraSdr, Driver::HydraSdr);

impl RxDevice for HydraSdr {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "hydrasdr",
                "invalid HydraSDR argument",
            ));
        }
        let stream = self
            .device
            .lock()
            .unwrap()
            .rx_stream()
            .map_err(map_hydrasdr_error)?;
        Ok(RxStreamer::new(stream))
    }
}

impl AntennaControl for HydraSdr {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        HydraSdr::antennas(self, direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        HydraSdr::antenna(self, direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        HydraSdr::set_antenna(self, direction, channel, name)
    }
}

impl AgcControl for HydraSdr {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        HydraSdr::agc_available(self, direction, channel)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        HydraSdr::set_agc_enabled(self, direction, channel, agc)
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        HydraSdr::agc_enabled(self, direction, channel)
    }
}

impl GainControl for HydraSdr {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        HydraSdr::gain_elements(self, direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        HydraSdr::set_gain(self, direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        HydraSdr::gain(self, direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HydraSdr::gain_range(self, direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        HydraSdr::set_gain_element(self, direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        HydraSdr::gain_element(self, direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        HydraSdr::gain_element_range(self, direction, channel, name)
    }
}

impl FrequencyControl for HydraSdr {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HydraSdr::frequency_range(self, direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        HydraSdr::frequency(self, direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        HydraSdr::set_frequency(self, direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        HydraSdr::frequency_components(self, direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        HydraSdr::component_frequency_range(self, direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        HydraSdr::component_frequency(self, direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        HydraSdr::set_component_frequency(self, direction, channel, name, frequency)
    }
}

impl SampleRateControl for HydraSdr {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        HydraSdr::sample_rate(self, direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        HydraSdr::set_sample_rate(self, direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HydraSdr::get_sample_rate_range(self, direction, channel)
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
            self.stream.stop().wait().map_err(map_hydrasdr_error)?;
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
        self.stream.start().wait().map_err(map_hydrasdr_error)?;
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
            .map_err(map_hydrasdr_error)
    }
}

fn open_selected_device(selector: DeviceSelector) -> Result<(HydraSdrDevice, u64), Error> {
    match selector {
        DeviceSelector::First => HydraSdrDevice::builder()
            .decimation_policy(DecimationPolicy::HighDefinition)
            .open()
            .wait()
            .map(|dev| {
                let serial = dev.info().serial.normalized();
                (dev, serial)
            })
            .map_err(map_hydrasdr_error),
        DeviceSelector::Serial(serial) => HydraSdrDevice::builder()
            .serial(serial)
            .decimation_policy(DecimationPolicy::HighDefinition)
            .open()
            .wait()
            .map(|dev| (dev, serial))
            .map_err(map_hydrasdr_error),
        DeviceSelector::Index(index) => {
            let devices = HydraSdrDevice::list().wait().map_err(map_hydrasdr_error)?;
            let Some(info) = devices.get(index) else {
                return Err(Error::DeviceNotFound);
            };
            let serial = info.serial.normalized();
            HydraSdrDevice::builder()
                .serial(serial)
                .decimation_policy(DecimationPolicy::HighDefinition)
                .open()
                .wait()
                .map(|dev| (dev, serial))
                .map_err(map_hydrasdr_error)
        }
    }
}
