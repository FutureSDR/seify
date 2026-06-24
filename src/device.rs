#![allow(dead_code)]
#![allow(unused_variables)]
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

use crate::Args;
use crate::Direction;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RxStreamer;
use crate::TxStreamer;

/// Type-erased RX streamer.
pub type DynRxStreamer = Box<dyn RxStreamer>;

/// Type-erased TX streamer.
pub type DynTxStreamer = Box<dyn TxStreamer>;

/// Runtime-dispatched device backend.
///
/// This is the backend trait behind [`DynDevice`]. It intentionally uses the
/// `Dyn` prefix because it is backed by `dyn` trait objects. `Any` names remain
/// reserved for APIs that directly involve [`std::any::Any`].
pub trait DynDeviceBackend: Send + Sync {
    /// Cast to [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Cast to [`Any`] for mutable downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// SDR [driver](Driver).
    fn driver(&self) -> Driver;
    /// Identifier for the device, e.g. its serial.
    fn id(&self) -> Result<String, Error>;
    /// Device info that can be displayed to the user.
    fn info(&self) -> Result<Args, Error>;

    /// Return a structured snapshot of the device's runtime capabilities.
    fn capabilities(&self) -> Result<DeviceCapabilities, Error> {
        DeviceCapabilities::from_dyn(self)
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        match direction {
            Direction::Rx => Ok(0),
            Direction::Tx => Ok(0),
        }
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        let _ = (direction, channel);
        Ok(false)
    }

    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<DynRxStreamer, Error> {
        let _ = (channels, args);
        Err(Error::NotSupported)
    }

    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<DynTxStreamer, Error> {
        let _ = (channels, args);
        Err(Error::NotSupported)
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        let _ = (direction, channel, name);
        Err(Error::NotSupported)
    }

    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        let _ = (direction, channel, agc);
        Err(Error::NotSupported)
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        let _ = (direction, channel, gain);
        Err(Error::NotSupported)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let _ = (direction, channel, name, gain);
        Err(Error::NotSupported)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        let _ = (direction, channel, name);
        Err(Error::NotSupported)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let _ = (direction, channel, name);
        Err(Error::NotSupported)
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        let _ = (direction, channel, frequency, args);
        Err(Error::NotSupported)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let _ = (direction, channel, name);
        Err(Error::NotSupported)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        let _ = (direction, channel, name);
        Err(Error::NotSupported)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        let _ = (direction, channel, name, frequency);
        Err(Error::NotSupported)
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let _ = (direction, channel, rate);
        Err(Error::NotSupported)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        let _ = (direction, channel, bw);
        Err(Error::NotSupported)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }

    fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error> {
        let _ = (direction, channel, automatic);
        Err(Error::NotSupported)
    }

    fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        let _ = (direction, channel);
        Err(Error::NotSupported)
    }
}

impl<T> DynDeviceBackend for T
where
    T: DeviceInfo
        + ChannelInfo
        + RxDevice
        + TxDevice
        + AntennaControl
        + AgcControl
        + GainControl
        + FrequencyControl
        + SampleRateControl
        + BandwidthControl
        + DcOffsetControl
        + Send
        + Sync,
    T::RxStreamer: 'static,
    T::TxStreamer: 'static,
{
    fn as_any(&self) -> &dyn Any {
        DeviceInfo::as_any(self)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        DeviceInfo::as_any_mut(self)
    }

    fn driver(&self) -> Driver {
        DeviceInfo::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        DeviceInfo::id(self)
    }

    fn info(&self) -> Result<Args, Error> {
        DeviceInfo::info(self)
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        ChannelInfo::num_channels(self, direction)
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        ChannelInfo::full_duplex(self, direction, channel)
    }

    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<DynRxStreamer, Error> {
        Ok(Box::new(RxDevice::rx_streamer(self, channels, args)?))
    }

    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<DynTxStreamer, Error> {
        Ok(Box::new(TxDevice::tx_streamer(self, channels, args)?))
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        AntennaControl::antennas(self, direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        AntennaControl::antenna(self, direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        AntennaControl::set_antenna(self, direction, channel, name)
    }

    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        AgcControl::supports_agc(self, direction, channel)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        AgcControl::enable_agc(self, direction, channel, agc)
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        AgcControl::agc(self, direction, channel)
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        GainControl::gain_elements(self, direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        GainControl::set_gain(self, direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        GainControl::gain(self, direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        GainControl::gain_range(self, direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        GainControl::set_gain_element(self, direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        GainControl::gain_element(self, direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        GainControl::gain_element_range(self, direction, channel, name)
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        FrequencyControl::frequency_range(self, direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        FrequencyControl::frequency(self, direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        FrequencyControl::set_frequency(self, direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        FrequencyControl::frequency_components(self, direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        FrequencyControl::component_frequency_range(self, direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        FrequencyControl::component_frequency(self, direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        FrequencyControl::set_component_frequency(self, direction, channel, name, frequency)
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        SampleRateControl::sample_rate(self, direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        SampleRateControl::set_sample_rate(self, direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        SampleRateControl::get_sample_rate_range(self, direction, channel)
    }

    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        BandwidthControl::bandwidth(self, direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        BandwidthControl::set_bandwidth(self, direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BandwidthControl::get_bandwidth_range(self, direction, channel)
    }

    fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        DcOffsetControl::has_dc_offset_mode(self, direction, channel)
    }

    fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error> {
        DcOffsetControl::set_dc_offset_mode(self, direction, channel, automatic)
    }

    fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        DcOffsetControl::dc_offset_mode(self, direction, channel)
    }
}

/// Runtime-dispatched device implementation.
///
/// This is usually used to create a hardware-independent `Device<DynDevice>`,
/// for example through [`Device::new`], when the concrete implementation is not
/// known at compile time.
pub type DynDevice = Arc<dyn DynDeviceBackend>;

/// Structured runtime capabilities for a device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    /// RX channels exposed by this device.
    pub rx_channels: Vec<ChannelCapabilities>,
    /// TX channels exposed by this device.
    pub tx_channels: Vec<ChannelCapabilities>,
}

impl DeviceCapabilities {
    /// Build a capability snapshot from a runtime-dispatched backend.
    pub fn from_dyn<D: DynDeviceBackend + ?Sized>(dev: &D) -> Result<Self, Error> {
        Ok(Self {
            rx_channels: channel_capabilities(dev, Direction::Rx)?,
            tx_channels: channel_capabilities(dev, Direction::Tx)?,
        })
    }
}

/// Structured runtime capabilities for one channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    /// Channel direction.
    pub direction: Direction,
    /// Channel index within its direction.
    pub channel: usize,
    /// Full-duplex support for this channel, if the backend exposes it.
    pub full_duplex: Option<bool>,
    /// Optional controls exposed by this channel.
    pub controls: ChannelControls,
}

/// Optional controls exposed by one channel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelControls {
    /// Available antenna ports.
    pub antennas: Option<Vec<String>>,
    /// Automatic gain control support.
    pub agc: Option<bool>,
    /// Named gain elements.
    pub gain_elements: Option<Vec<String>>,
    /// Overall gain range.
    pub gain_range: Option<Range>,
    /// Named frequency components.
    pub frequency_components: Option<Vec<String>>,
    /// Overall frequency range.
    pub frequency_range: Option<Range>,
    /// Baseband sample-rate range.
    pub sample_rate_range: Option<Range>,
    /// Hardware bandwidth range.
    pub bandwidth_range: Option<Range>,
    /// Automatic DC offset correction support.
    pub dc_offset_mode: Option<bool>,
}

fn channel_capabilities<D: DynDeviceBackend + ?Sized>(
    dev: &D,
    direction: Direction,
) -> Result<Vec<ChannelCapabilities>, Error> {
    let channels = match dev.num_channels(direction) {
        Ok(channels) => channels,
        Err(Error::NotSupported) => 0,
        Err(e) => return Err(e),
    };

    (0..channels)
        .map(|channel| {
            Ok(ChannelCapabilities {
                direction,
                channel,
                full_duplex: optional_capability(dev.full_duplex(direction, channel))?,
                controls: ChannelControls {
                    antennas: optional_capability(dev.antennas(direction, channel))?,
                    agc: optional_capability(dev.supports_agc(direction, channel))?,
                    gain_elements: optional_capability(dev.gain_elements(direction, channel))?,
                    gain_range: optional_capability(dev.gain_range(direction, channel))?,
                    frequency_components: optional_capability(
                        dev.frequency_components(direction, channel),
                    )?,
                    frequency_range: optional_capability(dev.frequency_range(direction, channel))?,
                    sample_rate_range: optional_capability(
                        dev.get_sample_rate_range(direction, channel),
                    )?,
                    bandwidth_range: optional_capability(
                        dev.get_bandwidth_range(direction, channel),
                    )?,
                    dc_offset_mode: optional_capability(
                        dev.has_dc_offset_mode(direction, channel),
                    )?,
                },
            })
        })
        .collect()
}

fn optional_capability<T>(result: Result<T, Error>) -> Result<Option<T>, Error> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(Error::NotSupported) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Basic device metadata.
pub trait DeviceInfo {
    /// Cast to [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Cast to [`Any`] for mutable downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// SDR [driver](Driver).
    fn driver(&self) -> Driver;
    /// Identifier for the device, e.g. its serial.
    fn id(&self) -> Result<String, Error>;
    /// Device info that can be displayed to the user.
    fn info(&self) -> Result<Args, Error>;
}

/// Basic channel metadata.
pub trait ChannelInfo {
    /// Number of supported channels.
    fn num_channels(&self, direction: Direction) -> Result<usize, Error>;
    /// Full-duplex support.
    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
}

/// RX streaming capability.
pub trait RxDevice {
    /// RX streamer implementation.
    type RxStreamer: RxStreamer;

    /// Create an RX streamer.
    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error>;
}

/// TX streaming capability.
pub trait TxDevice {
    /// TX streamer implementation.
    type TxStreamer: TxStreamer;

    /// Create a TX streamer.
    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error>;
}

/// Antenna control capability.
pub trait AntennaControl {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error>;
    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error>;
    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error>;
}

/// Automatic gain control capability.
pub trait AgcControl {
    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error>;
    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
}

/// Gain control capability.
pub trait GainControl {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error>;
    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error>;
    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error>;
    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;
    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error>;
    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error>;
    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error>;
}

/// Frequency control capability.
pub trait FrequencyControl {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;
    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error>;
    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error>;
    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error>;
    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error>;
    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error>;
    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error>;
}

/// Sample-rate control capability.
pub trait SampleRateControl {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error>;
    fn set_sample_rate(&self, direction: Direction, channel: usize, rate: f64)
        -> Result<(), Error>;
    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;
}

/// Bandwidth control capability.
pub trait BandwidthControl {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error>;
    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error>;
    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;
}

/// Automatic DC offset correction capability.
pub trait DcOffsetControl {
    fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
    fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error>;
    fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
}

/// Wraps a driver implementation.
///
/// Implements a more ergonomic version of the backend APIs, e.g. using
/// `Into<Args>`, which would not be possible in traits.
#[derive(Clone)]
pub struct Device<T> {
    dev: T,
}

impl<T> Device<T> {
    /// Create a device from the device implementation.
    pub fn from_impl(dev: T) -> Self {
        Self { dev }
    }
}

impl Device<DynDevice> {
    /// Creates a [`DynDevice`] opening the first device discovered through
    /// [`enumerate`](crate::enumerate).
    pub fn new() -> Result<Self, Error> {
        let mut devs = crate::enumerate()?;
        if devs.is_empty() {
            return Err(Error::NotFound);
        }
        Self::from_args(devs.remove(0))
    }

    /// Create a runtime-dispatched device from a device implementation.
    pub fn dyn_from_impl<T: DynDeviceBackend + 'static>(dev: T) -> Self {
        Self { dev: Arc::new(dev) }
    }

    /// Creates a [`DynDevice`] opening the first device with a given `driver`, specified in
    /// the `args` or the first device discovered through [`enumerate`](crate::enumerate) that
    /// matches the args.
    pub fn from_args<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args.try_into().map_err(|_| Error::ValueError)?;
        let driver = match args.get::<Driver>("driver") {
            Ok(d) => Some(d),
            Err(Error::NotFound) => None,
            Err(e) => return Err(e),
        };
        #[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
        {
            if driver.is_none() || matches!(driver, Some(Driver::AaroniaHttp)) {
                match crate::impls::AaroniaHttp::open(&args) {
                    Ok(d) => return Ok(Device { dev: Arc::new(d) }),
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]
        {
            if driver.is_none() || matches!(driver, Some(Driver::BladeRf)) {
                match crate::impls::BladeRf::open(&args) {
                    Ok(d) => return Ok(Device { dev: Arc::new(d) }),
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]
        {
            if driver.is_none() || matches!(driver, Some(Driver::BladeRf)) {
                match crate::impls::BladeRf::open(&args) {
                    Ok(d) => return Ok(Device { dev: Arc::new(d) }),
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
        {
            if driver.is_none() || matches!(driver, Some(Driver::RtlSdr)) {
                match crate::impls::RtlSdr::open(&args) {
                    Ok(d) => return Ok(Device { dev: Arc::new(d) }),
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
        {
            if driver.is_none() || matches!(driver, Some(Driver::Soapy)) {
                match crate::impls::Soapy::open(&args) {
                    Ok(d) => return Ok(Device { dev: Arc::new(d) }),
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(all(feature = "hackrfone", not(target_arch = "wasm32")))]
        {
            if driver.is_none() || matches!(driver, Some(Driver::HackRf)) {
                match crate::impls::HackRfOne::open(&args) {
                    Ok(d) => return Ok(Device { dev: Arc::new(d) }),
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(all(feature = "hydrasdr", not(target_arch = "wasm32")))]
        {
            if driver.is_none() || matches!(driver, Some(Driver::HydraSdr)) {
                match crate::impls::HydraSdr::open(&args) {
                    Ok(d) => return Ok(Device { dev: Arc::new(d) }),
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(not(all(feature = "hydrasdr", not(target_arch = "wasm32"))))]
        {
            if matches!(driver, Some(Driver::HydraSdr)) {
                return Err(Error::FeatureNotEnabled);
            }
        }
        #[cfg(feature = "dummy")]
        {
            if driver.is_none() || matches!(driver, Some(Driver::Dummy)) {
                match crate::impls::Dummy::open(&args) {
                    Ok(d) => return Ok(Device { dev: Arc::new(d) }),
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        Err(Error::NotFound)
    }
}

impl<T: DeviceInfo> Device<T> {
    /// SDR [driver](Driver).
    pub fn driver(&self) -> Driver {
        self.dev.driver()
    }

    /// Identifier for the device, e.g. its serial.
    pub fn id(&self) -> Result<String, Error> {
        self.dev.id()
    }

    /// Device info that can be displayed to the user.
    pub fn info(&self) -> Result<Args, Error> {
        self.dev.info()
    }

    /// Try to downcast to a given device implementation `D`, either directly (from `Device<D>`)
    /// or indirectly (from a `Device<DynDevice>` that wraps a `D`).
    pub fn impl_ref<D: DeviceInfo + 'static>(&self) -> Result<&D, Error> {
        if let Some(d) = self.dev.as_any().downcast_ref::<D>() {
            return Ok(d);
        }

        let d = self
            .dev
            .as_any()
            .downcast_ref::<DynDevice>()
            .ok_or(Error::ValueError)?;

        d.as_ref()
            .as_any()
            .downcast_ref::<D>()
            .ok_or(Error::ValueError)
    }

    /// Try to downcast mutably to a given device implementation `D`, either directly
    /// (from `Device<D>`) or indirectly (from a `Device<DynDevice>` that wraps a `D`).
    pub fn impl_mut<D: DeviceInfo + 'static>(&mut self) -> Result<&mut D, Error> {
        // work around borrow checker limitation
        if let Some(d) = self.dev.as_any().downcast_ref::<D>() {
            Ok(self.dev.as_any_mut().downcast_mut::<D>().unwrap())
        } else {
            let d = self
                .dev
                .as_any_mut()
                .downcast_mut::<DynDevice>()
                .ok_or(Error::ValueError)?;

            let d = Arc::get_mut(d).ok_or(Error::ValueError)?;
            d.as_any_mut().downcast_mut::<D>().ok_or(Error::ValueError)
        }
    }
}

impl DeviceInfo for DynDevice {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn driver(&self) -> Driver {
        self.as_ref().driver()
    }
    fn id(&self) -> Result<String, Error> {
        self.as_ref().id()
    }
    fn info(&self) -> Result<Args, Error> {
        self.as_ref().info()
    }
}

impl ChannelInfo for DynDevice {
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        self.as_ref().num_channels(direction)
    }
    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().full_duplex(direction, channel)
    }
}

impl RxDevice for DynDevice {
    type RxStreamer = DynRxStreamer;

    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error> {
        self.as_ref().rx_streamer(channels, args)
    }
}

impl TxDevice for DynDevice {
    type TxStreamer = DynTxStreamer;

    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error> {
        self.as_ref().tx_streamer(channels, args)
    }
}

impl AntennaControl for DynDevice {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.as_ref().antennas(direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.as_ref().antenna(direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        self.as_ref().set_antenna(direction, channel, name)
    }
}

impl AgcControl for DynDevice {
    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().supports_agc(direction, channel)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        self.as_ref().enable_agc(direction, channel, agc)
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().agc(direction, channel)
    }
}

impl GainControl for DynDevice {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.as_ref().gain_elements(direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.as_ref().set_gain(direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.as_ref().gain(direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref().gain_range(direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        self.as_ref()
            .set_gain_element(direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        self.as_ref().gain_element(direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.as_ref().gain_element_range(direction, channel, name)
    }
}

impl FrequencyControl for DynDevice {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref().frequency_range(direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.as_ref().frequency(direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        self.as_ref()
            .set_frequency(direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.as_ref().frequency_components(direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.as_ref()
            .component_frequency_range(direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.as_ref().component_frequency(direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        self.as_ref()
            .set_component_frequency(direction, channel, name, frequency)
    }
}

impl SampleRateControl for DynDevice {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.as_ref().sample_rate(direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.as_ref().set_sample_rate(direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref().get_sample_rate_range(direction, channel)
    }
}

impl BandwidthControl for DynDevice {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.as_ref().bandwidth(direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        self.as_ref().set_bandwidth(direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref().get_bandwidth_range(direction, channel)
    }
}

impl DcOffsetControl for DynDevice {
    fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().has_dc_offset_mode(direction, channel)
    }

    fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error> {
        self.as_ref()
            .set_dc_offset_mode(direction, channel, automatic)
    }

    fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().dc_offset_mode(direction, channel)
    }
}

impl Device<DynDevice> {
    /// Structured runtime capabilities for the device.
    pub fn capabilities(&self) -> Result<DeviceCapabilities, Error> {
        self.dev.capabilities()
    }
}

impl<T: ChannelInfo> Device<T> {
    /// Number of supported channels.
    pub fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        self.dev.num_channels(direction)
    }

    /// Full-duplex support.
    pub fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.full_duplex(direction, channel)
    }
}

impl<T: RxDevice> Device<T> {
    /// Create an RX streamer.
    pub fn rx_streamer(&self, channels: &[usize]) -> Result<T::RxStreamer, Error> {
        self.dev.rx_streamer(channels, Args::new())
    }

    /// Create an RX streamer, using `args`.
    pub fn rx_streamer_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<T::RxStreamer, Error> {
        self.dev.rx_streamer(channels, args)
    }
}

impl<T: TxDevice> Device<T> {
    /// Create a TX streamer.
    pub fn tx_streamer(&self, channels: &[usize]) -> Result<T::TxStreamer, Error> {
        self.dev.tx_streamer(channels, Args::new())
    }

    /// Create a TX streamer, using `args`.
    pub fn tx_streamer_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<T::TxStreamer, Error> {
        self.dev.tx_streamer(channels, args)
    }
}

impl<T: AntennaControl> Device<T> {
    pub fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.dev.antennas(direction, channel)
    }

    pub fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.dev.antenna(direction, channel)
    }

    pub fn set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        self.dev.set_antenna(direction, channel, name)
    }
}

impl<T: AgcControl> Device<T> {
    pub fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.supports_agc(direction, channel)
    }

    pub fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        self.dev.enable_agc(direction, channel, agc)
    }

    pub fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.agc(direction, channel)
    }
}

impl<T: GainControl> Device<T> {
    pub fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.dev.gain_elements(direction, channel)
    }

    pub fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.dev.set_gain(direction, channel, gain)
    }

    pub fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.dev.gain(direction, channel)
    }

    pub fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.dev.gain_range(direction, channel)
    }

    pub fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        self.dev.set_gain_element(direction, channel, name, gain)
    }

    pub fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        self.dev.gain_element(direction, channel, name)
    }

    pub fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.dev.gain_element_range(direction, channel, name)
    }
}

impl<T: FrequencyControl> Device<T> {
    pub fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.dev.frequency_range(direction, channel)
    }

    pub fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.frequency(direction, channel)
    }

    pub fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
    ) -> Result<(), Error> {
        self.dev
            .set_frequency(direction, channel, frequency, Args::new())
    }

    pub fn set_frequency_with_args(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        self.dev.set_frequency(direction, channel, frequency, args)
    }

    pub fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.dev.frequency_components(direction, channel)
    }

    pub fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.dev.component_frequency_range(direction, channel, name)
    }

    pub fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.dev.component_frequency(direction, channel, name)
    }

    pub fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        self.dev
            .set_component_frequency(direction, channel, name, frequency)
    }
}

impl<T: SampleRateControl> Device<T> {
    pub fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.sample_rate(direction, channel)
    }

    pub fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.dev.set_sample_rate(direction, channel, rate)
    }

    pub fn get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        self.dev.get_sample_rate_range(direction, channel)
    }
}

impl<T: BandwidthControl> Device<T> {
    pub fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.bandwidth(direction, channel)
    }

    pub fn set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bw: f64,
    ) -> Result<(), Error> {
        self.dev.set_bandwidth(direction, channel, bw)
    }

    pub fn get_bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        self.dev.get_bandwidth_range(direction, channel)
    }
}

impl<T: DcOffsetControl> Device<T> {
    pub fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.has_dc_offset_mode(direction, channel)
    }

    pub fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error> {
        self.dev.set_dc_offset_mode(direction, channel, automatic)
    }

    pub fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.dc_offset_mode(direction, channel)
    }
}

#[cfg(all(test, feature = "dummy"))]
mod tests {
    use super::*;

    #[test]
    fn dyn_device_reports_capabilities() {
        let dummy = crate::impls::Dummy::open(Args::new()).unwrap();
        let dev: Device<DynDevice> = Device::dyn_from_impl(dummy);

        let capabilities = dev.capabilities().unwrap();

        assert_eq!(capabilities.rx_channels.len(), 1);
        assert_eq!(capabilities.tx_channels.len(), 1);

        let rx0 = &capabilities.rx_channels[0];
        assert_eq!(rx0.direction, Direction::Rx);
        assert_eq!(rx0.channel, 0);
        assert_eq!(rx0.full_duplex, Some(true));
        assert_eq!(rx0.controls.antennas, Some(vec!["A".to_string()]));
        assert_eq!(rx0.controls.agc, Some(true));
        assert_eq!(rx0.controls.gain_elements, Some(vec!["RF".to_string()]));
        assert_eq!(
            rx0.controls.frequency_components,
            Some(vec!["freq".to_string()])
        );
        assert_eq!(rx0.controls.dc_offset_mode, Some(false));
    }
}
