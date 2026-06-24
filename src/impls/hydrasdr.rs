//! HydraSDR RFOne driver.

use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hydrasdr_rs::{
    Bandwidth, Config, DecimationMode, Device as HydraSdrDevice, DeviceDescriptor, ErrorKind,
    GainConfig, GainPreset, RfPort, SampleFormat,
};
use num_complex::Complex32;

use crate::Direction::*;
use crate::{
    AgcControl, AntennaControl, Args, BandwidthControl, Capability, ChannelInfo, DeviceInfo,
    Direction, Driver, DynDeviceBackend, Error, FrequencyControl, GainControl, Range, RangeItem,
    RxDevice, SampleRateControl,
};

const MTU: usize = 262_144 / 8;
const DEFAULT_SAMPLE_RATE_MIN: f64 = 10_000.0;
const DEFAULT_BANDWIDTH_MIN: f64 = 1_000.0;

#[derive(Clone)]
/// HydraSDR RFOne device backend.
pub struct HydraSdr {
    dev: Arc<Mutex<Option<HydraSdrDevice>>>,
    serial: Option<u64>,
    inner: Arc<Mutex<Inner>>,
}

unsafe impl Send for HydraSdr {}
unsafe impl Sync for HydraSdr {}

struct Inner {
    antenna: &'static str,
    frequency: Option<f64>,
    sample_rate: Option<f64>,
    bandwidth: Option<f64>,
    sample_rates: Vec<u32>,
    bandwidths: Vec<u32>,
    gains: Vec<GainCache>,
    gain_config: GainConfig,
    agc: bool,
    active_rx_streams: usize,
    min_frequency: f64,
    max_frequency: f64,
}

#[derive(Clone)]
struct GainCache {
    name: &'static str,
    gain_type: GainType,
    value: f64,
    range: Range,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GainType {
    Lna,
    Mixer,
    Vga,
    Linearity,
    Sensitivity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeviceSelector {
    First,
    Serial(u64),
    Index(usize),
}

/// HydraSDR RFOne receive streamer.
pub struct RxStreamer {
    dev: Arc<Mutex<Option<HydraSdrDevice>>>,
    inner: Arc<Mutex<Inner>>,
    active: bool,
}

unsafe impl Send for RxStreamer {}

/// Placeholder transmit streamer for unsupported TX operations.
pub struct TxDummy;
unsafe impl Send for TxDummy {}

impl HydraSdr {
    /// Return descriptors for detected HydraSDR RFOne devices.
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let mut devs = Vec::new();
        for dev in HydraSdrDevice::list().map_err(map_hydrasdr_error)? {
            devs.push(probe_args_from_info(dev));
        }
        Ok(devs)
    }

    /// Open a HydraSDR RFOne device from arguments.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = device_selector(&args)?;
        let (mut dev, serial) = open_selected_device(selector)?;
        let sample_rates = dev.sample_rates().unwrap_or_default();
        let bandwidths = dev.bandwidths().unwrap_or_default();
        let info = dev.info().clone();
        let current_config = info.current_config.as_ref();
        let gains = default_gain_cache();
        let min_frequency = info.min_frequency as f64;
        let max_frequency = info.max_frequency as f64;

        Ok(Self {
            dev: Arc::new(Mutex::new(Some(dev))),
            serial,
            inner: Arc::new(Mutex::new(Inner {
                antenna: "ANT",
                frequency: current_config.map(|config| config.frequency_hz() as f64),
                sample_rate: current_config
                    .map(|config| config.sample_rate_hz() as f64)
                    .or_else(|| sample_rates.first().map(|rate| *rate as f64)),
                bandwidth: current_config
                    .and_then(|config| match config.bandwidth() {
                        Bandwidth::Auto => None,
                        Bandwidth::ManualHz(bandwidth) => Some(bandwidth as f64),
                    })
                    .or_else(|| bandwidths.first().map(|bandwidth| *bandwidth as f64)),
                sample_rates,
                bandwidths,
                gains,
                gain_config: GainConfig::Unchanged,
                agc: false,
                active_rx_streams: 0,
                min_frequency,
                max_frequency,
            })),
        })
    }

    fn ensure_rx_config_idle(&self) -> Result<(), Error> {
        if self.inner.lock().unwrap().active_rx_streams == 0 {
            Ok(())
        } else {
            Err(Error::Busy)
        }
    }
}

impl HydraSdr {
    fn driver(&self) -> Driver {
        Driver::HydraSdr
    }

    fn id(&self) -> Result<String, Error> {
        if let Some(serial) = self.serial {
            return Ok(serial.to_string());
        }

        let dev = self.dev.lock().unwrap();
        let dev = dev.as_ref().ok_or(Error::DeviceDisconnected)?;
        dev.info()
            .serial
            .map(|serial| serial.to_string())
            .ok_or_else(|| Error::unsupported(Capability::DeviceId))
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

    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(false)
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(["ANT", "CABLE1", "CABLE2"]
            .into_iter()
            .map(str::to_string)
            .collect())
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        check_rx(direction, channel)?;
        Ok(self.inner.lock().unwrap().antenna.to_string())
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        check_rx(direction, channel)?;
        self.ensure_rx_config_idle()?;
        let (name, _) = antenna_port(name).ok_or(Error::invalid_argument(
            "hydrasdr",
            "invalid HydraSDR argument",
        ))?;
        let mut inner = self.inner.lock().unwrap();
        let old = inner.antenna;
        inner.antenna = name;
        let mut dev = self.dev.lock().unwrap();
        let Some(dev) = dev.as_mut() else {
            inner.antenna = old;
            return Err(Error::DeviceDisconnected);
        };
        if let Err(err) = configure_device(dev, &inner) {
            inner.antenna = old;
            return Err(err);
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
        self.ensure_rx_config_idle()?;
        let mut inner = self.inner.lock().unwrap();
        let old_agc = inner.agc;
        let old_gain_config = inner.gain_config;
        inner.agc = agc;
        inner.gain_config = manual_gain_config(&inner);
        let mut dev = self.dev.lock().unwrap();
        let Some(dev) = dev.as_mut() else {
            inner.agc = old_agc;
            inner.gain_config = old_gain_config;
            return Err(Error::DeviceDisconnected);
        };
        if let Err(err) = configure_device(dev, &inner) {
            inner.agc = old_agc;
            inner.gain_config = old_gain_config;
            return Err(err);
        }
        Ok(())
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(self.inner.lock().unwrap().agc)
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(self
            .inner
            .lock()
            .unwrap()
            .gains
            .iter()
            .map(|gain| gain.name.to_string())
            .collect())
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.set_gain_element(direction, channel, "LINEARITY", gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.gain_element(direction, channel, "LINEARITY")
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.gain_element_range(direction, channel, "LINEARITY")
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

        self.ensure_rx_config_idle()?;
        let mut inner = self.inner.lock().unwrap();
        let old_gains = inner.gains.clone();
        let old_gain_config = inner.gain_config;
        if let Some(cached) = inner
            .gains
            .iter_mut()
            .find(|cached| cached.gain_type == gain_type)
        {
            cached.value = gain;
        }
        inner.gain_config = match gain_type {
            GainType::Linearity => GainConfig::Preset(GainPreset::Linearity(gain.round() as u8)),
            GainType::Sensitivity => {
                GainConfig::Preset(GainPreset::Sensitivity(gain.round() as u8))
            }
            GainType::Lna | GainType::Mixer | GainType::Vga => manual_gain_config(&inner),
        };
        let mut dev = self.dev.lock().unwrap();
        let Some(dev) = dev.as_mut() else {
            inner.gains = old_gains;
            inner.gain_config = old_gain_config;
            return Err(Error::DeviceDisconnected);
        };
        if let Err(err) = configure_device(dev, &inner) {
            inner.gains = old_gains;
            inner.gain_config = old_gain_config;
            return Err(err);
        }
        Ok(())
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
        Ok(Some(
            self.inner
                .lock()
                .unwrap()
                .gains
                .iter()
                .find(|cached| cached.gain_type == gain_type)
                .ok_or(Error::invalid_argument(
                    "hydrasdr",
                    "invalid HydraSDR argument",
                ))?
                .value,
        ))
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
        Ok(self
            .inner
            .lock()
            .unwrap()
            .gains
            .iter()
            .find(|cached| cached.gain_type == gain_type)
            .ok_or(Error::invalid_argument(
                "hydrasdr",
                "invalid HydraSDR argument",
            ))?
            .range
            .clone())
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
            let inner = self.inner.lock().unwrap();
            Ok(Range::new(vec![RangeItem::Interval(
                inner.min_frequency,
                inner.max_frequency,
            )]))
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
        self.inner
            .lock()
            .unwrap()
            .frequency
            .ok_or(Error::unsupported(Capability::DriverOperation))
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
        self.ensure_rx_config_idle()?;
        let mut inner = self.inner.lock().unwrap();
        let old = inner.frequency;
        inner.frequency = Some(frequency);
        let mut dev = self.dev.lock().unwrap();
        let Some(dev) = dev.as_mut() else {
            inner.frequency = old;
            return Err(Error::DeviceDisconnected);
        };
        if let Err(err) = configure_device(dev, &inner) {
            inner.frequency = old;
            return Err(err);
        }
        Ok(())
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        self.inner
            .lock()
            .unwrap()
            .sample_rate
            .ok_or(Error::unsupported(Capability::DriverOperation))
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
        self.ensure_rx_config_idle()?;
        let mut inner = self.inner.lock().unwrap();
        let old = inner.sample_rate;
        inner.sample_rate = Some(rate);
        let mut dev = self.dev.lock().unwrap();
        let Some(dev) = dev.as_mut() else {
            inner.sample_rate = old;
            return Err(Error::DeviceDisconnected);
        };
        if let Err(err) = configure_device(dev, &inner) {
            inner.sample_rate = old;
            return Err(err);
        }
        Ok(())
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        let rates = &self.inner.lock().unwrap().sample_rates;
        if rates.is_empty() {
            Ok(Range::new(vec![RangeItem::Interval(
                DEFAULT_SAMPLE_RATE_MIN,
                u32::MAX as f64,
            )]))
        } else {
            Ok(Range::new(
                rates
                    .iter()
                    .map(|rate| RangeItem::Value(*rate as f64))
                    .collect(),
            ))
        }
    }

    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        self.inner
            .lock()
            .unwrap()
            .bandwidth
            .ok_or(Error::unsupported(Capability::DriverOperation))
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        let range = self.get_bandwidth_range(direction, channel)?;
        if !range.contains(bw) {
            return Err(Error::out_of_range("bandwidth", range, bw));
        }
        self.ensure_rx_config_idle()?;
        let mut inner = self.inner.lock().unwrap();
        let old = inner.bandwidth;
        inner.bandwidth = Some(bw);
        let mut dev = self.dev.lock().unwrap();
        let Some(dev) = dev.as_mut() else {
            inner.bandwidth = old;
            return Err(Error::DeviceDisconnected);
        };
        if let Err(err) = configure_device(dev, &inner) {
            inner.bandwidth = old;
            return Err(err);
        }
        Ok(())
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        let bandwidths = &self.inner.lock().unwrap().bandwidths;
        if bandwidths.is_empty() {
            Ok(Range::new(vec![RangeItem::Interval(
                DEFAULT_BANDWIDTH_MIN,
                u32::MAX as f64,
            )]))
        } else {
            Ok(Range::new(
                bandwidths
                    .iter()
                    .map(|bandwidth| RangeItem::Value(*bandwidth as f64))
                    .collect(),
            ))
        }
    }
}

impl DeviceInfo for HydraSdr {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn driver(&self) -> Driver {
        HydraSdr::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        HydraSdr::id(self)
    }

    fn info(&self) -> Result<Args, Error> {
        HydraSdr::info(self)
    }
}

impl DynDeviceBackend for HydraSdr {
    fn channel_info(&self) -> Option<&dyn ChannelInfo> {
        Some(self)
    }

    fn rx_device(&self) -> Option<&dyn crate::ErasedRxDevice> {
        Some(self)
    }

    fn antenna_control(&self) -> Option<&dyn AntennaControl> {
        Some(self)
    }

    fn agc_control(&self) -> Option<&dyn AgcControl> {
        Some(self)
    }

    fn gain_control(&self) -> Option<&dyn GainControl> {
        Some(self)
    }

    fn frequency_control(&self) -> Option<&dyn FrequencyControl> {
        Some(self)
    }

    fn sample_rate_control(&self) -> Option<&dyn SampleRateControl> {
        Some(self)
    }

    fn bandwidth_control(&self) -> Option<&dyn BandwidthControl> {
        Some(self)
    }
}

impl ChannelInfo for HydraSdr {
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        HydraSdr::num_channels(self, direction)
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        HydraSdr::full_duplex(self, direction, channel)
    }
}

impl RxDevice for HydraSdr {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "hydrasdr",
                "invalid HydraSDR argument",
            ));
        }
        self.ensure_rx_config_idle()?;
        Ok(RxStreamer::new(
            Arc::clone(&self.dev),
            Arc::clone(&self.inner),
        ))
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

impl BandwidthControl for HydraSdr {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        HydraSdr::bandwidth(self, direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        HydraSdr::set_bandwidth(self, direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HydraSdr::get_bandwidth_range(self, direction, channel)
    }
}

impl RxStreamer {
    fn new(dev: Arc<Mutex<Option<HydraSdrDevice>>>, inner: Arc<Mutex<Inner>>) -> Self {
        Self {
            dev,
            inner,
            active: false,
        }
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(MTU)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        if self.active {
            return Ok(());
        }
        if self.dev.lock().unwrap().is_none() {
            return Err(Error::DeviceDisconnected);
        }
        self.active = true;
        self.inner.lock().unwrap().active_rx_streams += 1;
        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        if self.active {
            self.active = false;
            let mut inner = self.inner.lock().unwrap();
            inner.active_rx_streams = inner.active_rx_streams.saturating_sub(1);
        }
        Ok(())
    }

    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        if buffers.len() != 1 {
            return Err(Error::invalid_argument(
                "hydrasdr",
                "invalid HydraSDR argument",
            ));
        }
        if buffers[0].is_empty() {
            return Ok(0);
        }

        let out = &mut buffers[0];
        let timeout = if timeout_us < 0 {
            Duration::MAX
        } else {
            Duration::from_micros(timeout_us as u64)
        };
        let mut dev = self.dev.lock().unwrap();
        let device = dev.as_mut().ok_or(Error::DeviceDisconnected)?;
        let mut stream = device.f32_rx_stream().map_err(map_hydrasdr_error)?;
        let mut iq = vec![(0.0, 0.0); out.len()];
        let read = stream.read(&mut iq, timeout).map_err(map_hydrasdr_error)?;
        stream.finish().map_err(map_hydrasdr_error)?;
        for (dst, (i, q)) in out.iter_mut().take(read).zip(iq) {
            *dst = Complex32::new(i, q);
        }
        Ok(read)
    }
}

impl Drop for RxStreamer {
    fn drop(&mut self) {
        let _ = <Self as crate::RxStreamer>::deactivate_at(self, None);
    }
}

impl crate::TxStreamer for TxDummy {
    fn mtu(&self) -> Result<usize, Error> {
        unreachable!()
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }

    fn write(
        &mut self,
        _buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        unreachable!()
    }

    fn write_all(
        &mut self,
        _buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<(), Error> {
        unreachable!()
    }
}

fn check_rx(direction: Direction, channel: usize) -> Result<(), Error> {
    if matches!(direction, Rx) && channel == 0 {
        Ok(())
    } else if matches!(direction, Rx) {
        Err(Error::invalid_channel(Direction::Rx, channel, 1))
    } else {
        Err(Error::unsupported(Capability::RxStreaming))
    }
}

fn antenna_port(name: &str) -> Option<(&'static str, RfPort)> {
    match name.to_ascii_uppercase().as_str() {
        "ANT" => Some(("ANT", RfPort::Rx0)),
        "CABLE1" => Some(("CABLE1", RfPort::Rx1)),
        "CABLE2" => Some(("CABLE2", RfPort::Rx2)),
        _ => None,
    }
}

fn gain_type(name: &str) -> Option<GainType> {
    match name.to_ascii_uppercase().as_str() {
        "LNA" => Some(GainType::Lna),
        "MIXER" => Some(GainType::Mixer),
        "VGA" => Some(GainType::Vga),
        "LINEARITY" => Some(GainType::Linearity),
        "SENSITIVITY" => Some(GainType::Sensitivity),
        _ => None,
    }
}

fn default_gain_cache() -> Vec<GainCache> {
    [
        ("LNA", GainType::Lna, 0, 14, 8),
        ("MIXER", GainType::Mixer, 0, 15, 8),
        ("VGA", GainType::Vga, 0, 15, 8),
        ("LINEARITY", GainType::Linearity, 0, 21, 10),
        ("SENSITIVITY", GainType::Sensitivity, 0, 21, 10),
    ]
    .into_iter()
    .map(|(name, gain_type, min_value, max_value, value)| {
        gain_cache_item(name, gain_type, min_value, max_value, 1, value)
    })
    .collect()
}

fn probe_args_from_info(dev: DeviceDescriptor) -> Args {
    let mut args = Args::default();
    args.set("driver", "hydrasdr");
    args.set("vid", format!("0x{:04x}", dev.vid));
    args.set("pid", format!("0x{:04x}", dev.pid));
    args.set("description", dev.description);
    if let Some(serial) = dev.serial {
        args.set("serial", serial.to_string());
    }
    if let Some(product) = dev.product_string {
        args.set("product", product);
    }
    args
}

fn device_selector(args: &Args) -> Result<DeviceSelector, Error> {
    match args.get::<usize>("index") {
        Ok(index) => return Ok(DeviceSelector::Index(index)),
        Err(Error::MissingArgument { .. }) => {}
        Err(err) => return Err(err),
    }

    match args.get::<u64>("serial") {
        Ok(serial) => Ok(DeviceSelector::Serial(serial)),
        Err(Error::MissingArgument { .. }) => Ok(DeviceSelector::First),
        Err(err) => Err(err),
    }
}

fn open_selected_device(selector: DeviceSelector) -> Result<(HydraSdrDevice, Option<u64>), Error> {
    match selector {
        DeviceSelector::First => HydraSdrDevice::builder()
            .sample_format(SampleFormat::F32Iq)
            .decimation_mode(DecimationMode::HighDefinition)
            .open()
            .map(|dev| {
                let serial = dev.info().serial;
                (dev, serial)
            })
            .map_err(map_hydrasdr_error),
        DeviceSelector::Serial(serial) => HydraSdrDevice::builder()
            .serial(serial)
            .sample_format(SampleFormat::F32Iq)
            .decimation_mode(DecimationMode::HighDefinition)
            .open()
            .map(|dev| (dev, Some(serial)))
            .map_err(map_hydrasdr_error),
        DeviceSelector::Index(index) => {
            let devices = HydraSdrDevice::list().map_err(map_hydrasdr_error)?;
            let Some(info) = devices.get(index) else {
                return Err(Error::DeviceNotFound);
            };
            if let Some(serial) = info.serial {
                HydraSdrDevice::builder()
                    .serial(serial)
                    .sample_format(SampleFormat::F32Iq)
                    .decimation_mode(DecimationMode::HighDefinition)
                    .open()
                    .map(|dev| (dev, Some(serial)))
                    .map_err(map_hydrasdr_error)
            } else if index == 0 {
                HydraSdrDevice::builder()
                    .sample_format(SampleFormat::F32Iq)
                    .decimation_mode(DecimationMode::HighDefinition)
                    .open()
                    .map(|dev| {
                        let serial = dev.info().serial;
                        (dev, serial)
                    })
                    .map_err(map_hydrasdr_error)
            } else {
                Err(Error::DeviceNotFound)
            }
        }
    }
}

fn configure_device(dev: &mut HydraSdrDevice, inner: &Inner) -> Result<(), Error> {
    let (_, port) = antenna_port(inner.antenna).ok_or(Error::invalid_argument(
        "hydrasdr",
        "invalid HydraSDR argument",
    ))?;
    let mut builder = Config::builder()
        .sample_format(SampleFormat::F32Iq)
        .decimation_mode(DecimationMode::HighDefinition)
        .rf_port(port)
        .gain(inner.gain_config)
        .packing(false);
    if let Some(frequency) = inner.frequency {
        builder = builder.frequency_hz(frequency as u64);
    }
    if let Some(sample_rate) = inner.sample_rate {
        builder = builder.sample_rate_hz(sample_rate as u32);
    }
    if let Some(bandwidth) = inner.bandwidth {
        builder = builder.bandwidth(Bandwidth::ManualHz(bandwidth as u32));
    }
    let config = builder.build().map_err(map_hydrasdr_error)?;
    dev.configure(&config).map_err(map_hydrasdr_error)
}

fn manual_gain_config(inner: &Inner) -> GainConfig {
    GainConfig::Manual {
        lna: cached_gain_value(inner, GainType::Lna),
        mixer: cached_gain_value(inner, GainType::Mixer),
        vga: cached_gain_value(inner, GainType::Vga),
        lna_agc: Some(inner.agc),
        mixer_agc: Some(inner.agc),
    }
}

fn cached_gain_value(inner: &Inner, gain_type: GainType) -> Option<u8> {
    inner
        .gains
        .iter()
        .find(|cached| cached.gain_type == gain_type)
        .map(|cached| cached.value.round() as u8)
}

fn gain_cache_item(
    name: &'static str,
    gain_type: GainType,
    min_value: u8,
    max_value: u8,
    step_value: u8,
    value: u8,
) -> GainCache {
    let step = step_value.max(1) as f64;
    GainCache {
        name,
        gain_type,
        value: value as f64,
        range: Range::new(vec![RangeItem::Step(
            min_value as f64,
            max_value as f64,
            step,
        )]),
    }
}

fn map_hydrasdr_error(err: hydrasdr_rs::Error) -> Error {
    match err.kind() {
        ErrorKind::InvalidConfig => Error::invalid_argument("hydrasdr", err.to_string()),
        ErrorKind::NotFound => Error::DeviceNotFound,
        ErrorKind::Busy => Error::Busy,
        ErrorKind::Unsupported => Error::unsupported(Capability::DriverOperation),
        ErrorKind::StreamClosed => Error::StreamClosed,
        _ => err.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_args_from_info_maps_usb_metadata_without_opening_hardware() {
        let info = DeviceDescriptor {
            vid: 0x38af,
            pid: 0x0001,
            description: "HydraSDR RFOne Official VID/PID",
            serial: Some(0x1234_5678_9abc_def0),
            product_string: Some("HydraSDR RFOne".to_string()),
        };

        let args = probe_args_from_info(info);

        assert_eq!(args.get::<String>("driver").unwrap(), "hydrasdr");
        assert_eq!(args.get::<String>("vid").unwrap(), "0x38af");
        assert_eq!(args.get::<String>("pid").unwrap(), "0x0001");
        assert_eq!(
            args.get::<String>("description").unwrap(),
            "HydraSDR RFOne Official VID/PID"
        );
        assert_eq!(args.get::<String>("serial").unwrap(), "1311768467463790320");
        assert_eq!(args.get::<String>("product").unwrap(), "HydraSDR RFOne");
    }

    #[test]
    fn check_rx_accepts_only_rx_channel_zero_and_rejects_tx() {
        assert!(check_rx(Rx, 0).is_ok());
        assert!(matches!(
            check_rx(Rx, 1),
            Err(Error::InvalidChannel {
                direction: Rx,
                channel: 1,
                available: 1,
            })
        ));
        assert!(matches!(
            check_rx(Tx, 0),
            Err(Error::Unsupported {
                capability: Capability::RxStreaming,
                ..
            })
        ));
    }

    #[test]
    fn device_selector_defaults_to_first_device() {
        let args = Args::default();

        assert_eq!(device_selector(&args).unwrap(), DeviceSelector::First);
    }

    #[test]
    fn device_selector_accepts_serial() {
        let args: Args = "driver=hydrasdr,serial=1234".try_into().unwrap();

        assert_eq!(
            device_selector(&args).unwrap(),
            DeviceSelector::Serial(1234)
        );
    }

    #[test]
    fn device_selector_prefers_index_over_serial_like_other_seify_drivers() {
        let args: Args = "driver=hydrasdr,index=2,serial=1234".try_into().unwrap();

        assert_eq!(device_selector(&args).unwrap(), DeviceSelector::Index(2));
    }

    #[test]
    fn device_selector_rejects_invalid_index_and_serial_args() {
        let bad_index: Args = "driver=hydrasdr,index=not-a-number".try_into().unwrap();
        assert!(matches!(
            device_selector(&bad_index),
            Err(Error::InvalidArgument { name, .. }) if name == "index"
        ));

        let bad_serial: Args = "driver=hydrasdr,serial=not-a-number".try_into().unwrap();
        assert!(matches!(
            device_selector(&bad_serial),
            Err(Error::InvalidArgument { name, .. }) if name == "serial"
        ));
    }
}
