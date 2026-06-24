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

/// Object-safe RX streaming capability for runtime-dispatched devices.
pub trait ErasedRxDevice {
    /// Create a type-erased RX streamer.
    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<DynRxStreamer, Error>;
}

impl<T> ErasedRxDevice for T
where
    T: RxDevice,
    T::RxStreamer: 'static,
{
    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<DynRxStreamer, Error> {
        Ok(Box::new(RxDevice::rx_streamer(self, channels, args)?))
    }
}

/// Object-safe TX streaming capability for runtime-dispatched devices.
pub trait ErasedTxDevice {
    /// Create a type-erased TX streamer.
    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<DynTxStreamer, Error>;
}

impl<T> ErasedTxDevice for T
where
    T: TxDevice,
    T::TxStreamer: 'static,
{
    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<DynTxStreamer, Error> {
        Ok(Box::new(TxDevice::tx_streamer(self, channels, args)?))
    }
}

/// Runtime-dispatched device backend.
///
/// The erased backend only exposes device metadata and optional views into
/// capability traits. Individual controls live on the smaller capability traits
/// instead of one mandatory universal device interface.
pub trait DynDeviceBackend: DeviceInfo + Send + Sync {
    /// Return a structured snapshot of the device's runtime capabilities.
    fn capabilities(&self) -> Result<DeviceCapabilities, Error> {
        DeviceCapabilities::from_dyn(self)
    }

    fn channel_info(&self) -> Option<&dyn ChannelInfo> {
        None
    }

    fn rx_device(&self) -> Option<&dyn ErasedRxDevice> {
        None
    }

    fn tx_device(&self) -> Option<&dyn ErasedTxDevice> {
        None
    }

    fn antenna_control(&self) -> Option<&dyn AntennaControl> {
        None
    }

    fn agc_control(&self) -> Option<&dyn AgcControl> {
        None
    }

    fn gain_control(&self) -> Option<&dyn GainControl> {
        None
    }

    fn frequency_control(&self) -> Option<&dyn FrequencyControl> {
        None
    }

    fn sample_rate_control(&self) -> Option<&dyn SampleRateControl> {
        None
    }

    fn bandwidth_control(&self) -> Option<&dyn BandwidthControl> {
        None
    }

    fn dc_offset_control(&self) -> Option<&dyn DcOffsetControl> {
        None
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
    pub agc: bool,
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
    pub dc_offset: bool,
}

fn channel_capabilities<D: DynDeviceBackend + ?Sized>(
    dev: &D,
    direction: Direction,
) -> Result<Vec<ChannelCapabilities>, Error> {
    let Some(channel_info) = dev.channel_info() else {
        return Ok(Vec::new());
    };
    let channels = match channel_info.num_channels(direction) {
        Ok(channels) => channels,
        Err(Error::NotSupported) => 0,
        Err(e) => return Err(e),
    };

    (0..channels)
        .map(|channel| {
            Ok(ChannelCapabilities {
                direction,
                channel,
                full_duplex: optional_capability(channel_info.full_duplex(direction, channel))?,
                controls: ChannelControls {
                    antennas: optional_erased_capability(dev.antenna_control(), |cap| {
                        cap.antennas(direction, channel)
                    })?,
                    agc: erased_capability_available(dev.agc_control(), |cap| {
                        cap.agc_available(direction, channel)
                    })?,
                    gain_elements: optional_erased_capability(dev.gain_control(), |cap| {
                        cap.gain_elements(direction, channel)
                    })?,
                    gain_range: optional_erased_capability(dev.gain_control(), |cap| {
                        cap.gain_range(direction, channel)
                    })?,
                    frequency_components: optional_erased_capability(
                        dev.frequency_control(),
                        |cap| cap.frequency_components(direction, channel),
                    )?,
                    frequency_range: optional_erased_capability(dev.frequency_control(), |cap| {
                        cap.frequency_range(direction, channel)
                    })?,
                    sample_rate_range: optional_erased_capability(
                        dev.sample_rate_control(),
                        |cap| cap.get_sample_rate_range(direction, channel),
                    )?,
                    bandwidth_range: optional_erased_capability(dev.bandwidth_control(), |cap| {
                        cap.get_bandwidth_range(direction, channel)
                    })?,
                    dc_offset: erased_capability_available(dev.dc_offset_control(), |cap| {
                        cap.dc_offset_available(direction, channel)
                    })?,
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

fn optional_erased_capability<C: ?Sized, T>(
    cap: Option<&C>,
    f: impl FnOnce(&C) -> Result<T, Error>,
) -> Result<Option<T>, Error> {
    match cap {
        Some(cap) => optional_capability(f(cap)),
        None => Ok(None),
    }
}

fn erased_capability_available<C: ?Sized>(
    cap: Option<&C>,
    f: impl FnOnce(&C) -> Result<bool, Error>,
) -> Result<bool, Error> {
    match cap {
        Some(cap) => match f(cap) {
            Ok(available) => Ok(available),
            Err(Error::NotSupported) => Ok(false),
            Err(e) => Err(e),
        },
        None => Ok(false),
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
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error>;
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
    fn dc_offset_available(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
    fn dc_offset_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
    fn set_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error>;
}

/// Handle for automatic gain control on one device channel.
pub struct Agc<'a, T: AgcControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AgcControl + ?Sized> Agc<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Result<Self, Error> {
        if !dev.agc_available(direction, channel)? {
            return Err(Error::NotSupported);
        }
        Ok(Self {
            dev,
            direction,
            channel,
        })
    }

    /// Return whether automatic gain control is enabled.
    pub fn enabled(&self) -> Result<bool, Error> {
        self.dev.agc_enabled(self.direction, self.channel)
    }

    /// Enable automatic gain control.
    pub fn enable(&self) -> Result<(), Error> {
        self.set_enabled(true)
    }

    /// Disable automatic gain control.
    pub fn disable(&self) -> Result<(), Error> {
        self.set_enabled(false)
    }

    /// Set whether automatic gain control is enabled.
    pub fn set_enabled(&self, enabled: bool) -> Result<(), Error> {
        self.dev
            .set_agc_enabled(self.direction, self.channel, enabled)
    }
}

/// Handle for automatic DC offset correction on one device channel.
pub struct DcOffset<'a, T: DcOffsetControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: DcOffsetControl + ?Sized> DcOffset<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Result<Self, Error> {
        if !dev.dc_offset_available(direction, channel)? {
            return Err(Error::NotSupported);
        }
        Ok(Self {
            dev,
            direction,
            channel,
        })
    }

    /// Return whether automatic DC offset correction is enabled.
    pub fn enabled(&self) -> Result<bool, Error> {
        self.dev.dc_offset_enabled(self.direction, self.channel)
    }

    /// Enable automatic DC offset correction.
    pub fn enable(&self) -> Result<(), Error> {
        self.set_enabled(true)
    }

    /// Disable automatic DC offset correction.
    pub fn disable(&self) -> Result<(), Error> {
        self.set_enabled(false)
    }

    /// Set whether automatic DC offset correction is enabled.
    pub fn set_enabled(&self, enabled: bool) -> Result<(), Error> {
        self.dev
            .set_dc_offset_enabled(self.direction, self.channel, enabled)
    }
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
        self.as_ref()
            .channel_info()
            .ok_or(Error::NotSupported)?
            .num_channels(direction)
    }
    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref()
            .channel_info()
            .ok_or(Error::NotSupported)?
            .full_duplex(direction, channel)
    }
}

impl RxDevice for DynDevice {
    type RxStreamer = DynRxStreamer;

    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error> {
        self.as_ref()
            .rx_device()
            .ok_or(Error::NotSupported)?
            .rx_streamer(channels, args)
    }
}

impl TxDevice for DynDevice {
    type TxStreamer = DynTxStreamer;

    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error> {
        self.as_ref()
            .tx_device()
            .ok_or(Error::NotSupported)?
            .tx_streamer(channels, args)
    }
}

impl AntennaControl for DynDevice {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.as_ref()
            .antenna_control()
            .ok_or(Error::NotSupported)?
            .antennas(direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.as_ref()
            .antenna_control()
            .ok_or(Error::NotSupported)?
            .antenna(direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        self.as_ref()
            .antenna_control()
            .ok_or(Error::NotSupported)?
            .set_antenna(direction, channel, name)
    }
}

impl GainControl for DynDevice {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.as_ref()
            .gain_control()
            .ok_or(Error::NotSupported)?
            .gain_elements(direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.as_ref()
            .gain_control()
            .ok_or(Error::NotSupported)?
            .set_gain(direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.as_ref()
            .gain_control()
            .ok_or(Error::NotSupported)?
            .gain(direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref()
            .gain_control()
            .ok_or(Error::NotSupported)?
            .gain_range(direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        self.as_ref()
            .gain_control()
            .ok_or(Error::NotSupported)?
            .set_gain_element(direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        self.as_ref()
            .gain_control()
            .ok_or(Error::NotSupported)?
            .gain_element(direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.as_ref()
            .gain_control()
            .ok_or(Error::NotSupported)?
            .gain_element_range(direction, channel, name)
    }
}

impl FrequencyControl for DynDevice {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref()
            .frequency_control()
            .ok_or(Error::NotSupported)?
            .frequency_range(direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.as_ref()
            .frequency_control()
            .ok_or(Error::NotSupported)?
            .frequency(direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        self.as_ref()
            .frequency_control()
            .ok_or(Error::NotSupported)?
            .set_frequency(direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.as_ref()
            .frequency_control()
            .ok_or(Error::NotSupported)?
            .frequency_components(direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.as_ref()
            .frequency_control()
            .ok_or(Error::NotSupported)?
            .component_frequency_range(direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.as_ref()
            .frequency_control()
            .ok_or(Error::NotSupported)?
            .component_frequency(direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        self.as_ref()
            .frequency_control()
            .ok_or(Error::NotSupported)?
            .set_component_frequency(direction, channel, name, frequency)
    }
}

impl SampleRateControl for DynDevice {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.as_ref()
            .sample_rate_control()
            .ok_or(Error::NotSupported)?
            .sample_rate(direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.as_ref()
            .sample_rate_control()
            .ok_or(Error::NotSupported)?
            .set_sample_rate(direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref()
            .sample_rate_control()
            .ok_or(Error::NotSupported)?
            .get_sample_rate_range(direction, channel)
    }
}

impl BandwidthControl for DynDevice {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.as_ref()
            .bandwidth_control()
            .ok_or(Error::NotSupported)?
            .bandwidth(direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        self.as_ref()
            .bandwidth_control()
            .ok_or(Error::NotSupported)?
            .set_bandwidth(direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref()
            .bandwidth_control()
            .ok_or(Error::NotSupported)?
            .get_bandwidth_range(direction, channel)
    }
}

impl Device<DynDevice> {
    /// Structured runtime capabilities for the device.
    pub fn capabilities(&self) -> Result<DeviceCapabilities, Error> {
        self.dev.capabilities()
    }

    /// Automatic gain control for one channel.
    pub fn agc(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Agc<'_, dyn AgcControl + '_>, Error> {
        Agc::new(
            self.dev.as_ref().agc_control().ok_or(Error::NotSupported)?,
            direction,
            channel,
        )
    }

    /// Automatic DC offset correction for one channel.
    pub fn dc_offset(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<DcOffset<'_, dyn DcOffsetControl + '_>, Error> {
        DcOffset::new(
            self.dev
                .as_ref()
                .dc_offset_control()
                .ok_or(Error::NotSupported)?,
            direction,
            channel,
        )
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
    /// Automatic gain control for one channel.
    pub fn agc(&self, direction: Direction, channel: usize) -> Result<Agc<'_, T>, Error> {
        Agc::new(&self.dev, direction, channel)
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
    /// Automatic DC offset correction for one channel.
    pub fn dc_offset(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<DcOffset<'_, T>, Error> {
        DcOffset::new(&self.dev, direction, channel)
    }
}

#[cfg(all(test, feature = "dummy"))]
mod tests {
    use super::*;

    struct RxOnly;

    struct DcToggle(std::sync::Mutex<bool>);

    struct TestRxStreamer;

    impl DeviceInfo for RxOnly {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn driver(&self) -> Driver {
            Driver::Dummy
        }

        fn id(&self) -> Result<String, Error> {
            Ok("rx-only".to_string())
        }

        fn info(&self) -> Result<Args, Error> {
            Ok(Args::new())
        }
    }

    impl DynDeviceBackend for RxOnly {
        fn channel_info(&self) -> Option<&dyn ChannelInfo> {
            Some(self)
        }

        fn rx_device(&self) -> Option<&dyn ErasedRxDevice> {
            Some(self)
        }
    }

    impl ChannelInfo for RxOnly {
        fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
            match direction {
                Direction::Rx => Ok(1),
                Direction::Tx => Ok(0),
            }
        }

        fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
            Ok(false)
        }
    }

    impl RxDevice for RxOnly {
        type RxStreamer = TestRxStreamer;

        fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
            match channels {
                &[0] => Ok(TestRxStreamer),
                _ => Err(Error::ValueError),
            }
        }
    }

    impl DcOffsetControl for DcToggle {
        fn dc_offset_available(
            &self,
            _direction: Direction,
            channel: usize,
        ) -> Result<bool, Error> {
            if channel == 0 {
                Ok(true)
            } else {
                Err(Error::ValueError)
            }
        }

        fn dc_offset_enabled(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
            if channel == 0 {
                Ok(*self.0.lock().unwrap())
            } else {
                Err(Error::ValueError)
            }
        }

        fn set_dc_offset_enabled(
            &self,
            _direction: Direction,
            channel: usize,
            enabled: bool,
        ) -> Result<(), Error> {
            if channel == 0 {
                *self.0.lock().unwrap() = enabled;
                Ok(())
            } else {
                Err(Error::ValueError)
            }
        }
    }

    impl crate::RxStreamer for TestRxStreamer {
        fn mtu(&self) -> Result<usize, Error> {
            Ok(1)
        }

        fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
            Ok(())
        }

        fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
            Ok(())
        }

        fn read(
            &mut self,
            _buffers: &mut [&mut [num_complex::Complex32]],
            _timeout_us: i64,
        ) -> Result<usize, Error> {
            Ok(0)
        }
    }

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
        assert!(rx0.controls.agc);
        assert_eq!(rx0.controls.gain_elements, Some(vec!["RF".to_string()]));
        assert_eq!(
            rx0.controls.frequency_components,
            Some(vec!["freq".to_string()])
        );
        assert!(!rx0.controls.dc_offset);
    }

    #[test]
    fn agc_handle_controls_enabled_state() {
        let dummy = crate::impls::Dummy::open(Args::new()).unwrap();
        let dev = Device::from_impl(dummy);
        let agc = dev.agc(Direction::Rx, 0).unwrap();

        agc.enable().unwrap();
        assert!(agc.enabled().unwrap());

        agc.disable().unwrap();
        assert!(!agc.enabled().unwrap());
    }

    #[test]
    fn dc_offset_handle_controls_enabled_state() {
        let dev = Device::from_impl(DcToggle(std::sync::Mutex::new(false)));
        let dc_offset = dev.dc_offset(Direction::Rx, 0).unwrap();

        dc_offset.enable().unwrap();
        assert!(dc_offset.enabled().unwrap());

        dc_offset.disable().unwrap();
        assert!(!dc_offset.enabled().unwrap());
    }

    #[test]
    fn dyn_device_does_not_require_all_capabilities() {
        let dev: Device<DynDevice> = Device::dyn_from_impl(RxOnly);

        let capabilities = dev.capabilities().unwrap();
        assert_eq!(capabilities.rx_channels.len(), 1);
        assert_eq!(capabilities.tx_channels.len(), 0);
        assert_eq!(
            capabilities.rx_channels[0].controls,
            ChannelControls::default()
        );

        assert!(dev.rx_streamer(&[0]).is_ok());
        assert!(matches!(dev.tx_streamer(&[0]), Err(Error::NotSupported)));
        assert!(matches!(
            dev.agc(Direction::Rx, 0),
            Err(Error::NotSupported)
        ));
        assert!(matches!(
            dev.dc_offset(Direction::Rx, 0),
            Err(Error::NotSupported)
        ));
    }
}
