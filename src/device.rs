#![allow(dead_code)]
#![allow(unused_variables)]
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

use crate::Args;
use crate::Capability;
use crate::Direction;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::Registry;
use crate::RxStreamer;
use crate::TxStreamer;
use crate::TypedDeviceBackend;

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

/// Runtime-dispatched opened device.
///
/// This is used to control a device when the concrete driver implementation is
/// not known at compile time.
#[derive(Clone)]
pub struct DynDevice {
    inner: Arc<dyn DynDeviceBackend>,
}

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

fn channel_capabilities<D>(dev: &D, direction: Direction) -> Result<Vec<ChannelCapabilities>, Error>
where
    D: DynDeviceBackend + ?Sized,
{
    let Some(channel_info) = dev.channel_info() else {
        return Ok(Vec::new());
    };
    let channels = match channel_info.num_channels(direction) {
        Ok(channels) => channels,
        Err(e) if e.is_unsupported() => 0,
        Err(e) => return Err(e),
    };

    (0..channels)
        .map(|channel| {
            Ok(ChannelCapabilities {
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
        Err(e) if e.is_unsupported() => Ok(None),
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
            Err(e) if e.is_unsupported() => Ok(false),
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

/// RX channel handle.
pub struct RxChannel<'a, T: ?Sized> {
    dev: &'a T,
    channel: usize,
}

impl<'a, T: ?Sized> RxChannel<'a, T> {
    fn new(dev: &'a T, channel: usize) -> Self {
        Self { dev, channel }
    }

    /// Channel index.
    pub fn id(&self) -> usize {
        self.channel
    }

    /// Channel index.
    pub fn index(&self) -> usize {
        self.channel
    }
}

/// TX channel handle.
pub struct TxChannel<'a, T: ?Sized> {
    dev: &'a T,
    channel: usize,
}

impl<'a, T: ?Sized> TxChannel<'a, T> {
    fn new(dev: &'a T, channel: usize) -> Self {
        Self { dev, channel }
    }

    /// Channel index.
    pub fn id(&self) -> usize {
        self.channel
    }

    /// Channel index.
    pub fn index(&self) -> usize {
        self.channel
    }
}

/// Antenna control handle for one channel.
pub struct Antenna<'a, T: AntennaControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
    ports: Vec<String>,
}

impl<'a, T> Antenna<'a, T>
where
    T: AntennaControl + ?Sized,
{
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Result<Self, Error> {
        let ports = dev.antennas(direction, channel)?;
        Ok(Self {
            dev,
            direction,
            channel,
            ports,
        })
    }

    /// Selectable antenna ports.
    pub fn ports(&self) -> &[String] {
        &self.ports
    }

    /// Currently selected antenna.
    pub fn selected(&self) -> Result<String, Error> {
        self.dev.antenna(self.direction, self.channel)
    }

    /// Select an antenna port.
    pub fn select(&self, name: &str) -> Result<(), Error> {
        self.dev.set_antenna(self.direction, self.channel, name)
    }
}

/// Automatic gain control handle for one channel.
pub struct Agc<'a, T: AgcControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T> Agc<'a, T>
where
    T: AgcControl + ?Sized,
{
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Result<Self, Error> {
        if !dev.agc_available(direction, channel)? {
            return Err(Error::unsupported(Capability::Agc));
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

/// Gain control handle for one channel.
pub struct Gain<'a, T: GainControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T> Gain<'a, T>
where
    T: GainControl + ?Sized,
{
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Named gain elements.
    pub fn elements(&self) -> Result<Vec<String>, Error> {
        self.dev.gain_elements(self.direction, self.channel)
    }

    /// Overall gain.
    pub fn value(&self) -> Result<Option<f64>, Error> {
        self.dev.gain(self.direction, self.channel)
    }

    /// Set overall gain.
    pub fn set(&self, gain: f64) -> Result<(), Error> {
        self.dev.set_gain(self.direction, self.channel, gain)
    }

    /// Overall gain range.
    pub fn range(&self) -> Result<Range, Error> {
        self.dev.gain_range(self.direction, self.channel)
    }

    /// Named gain element handle.
    pub fn element(&self, name: &str) -> GainElement<'a, T> {
        GainElement {
            dev: self.dev,
            direction: self.direction,
            channel: self.channel,
            name: name.to_string(),
        }
    }
}

/// Gain element handle for one channel.
pub struct GainElement<'a, T: GainControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
    name: String,
}

impl<'a, T> GainElement<'a, T>
where
    T: GainControl + ?Sized,
{
    /// Gain element value.
    pub fn value(&self) -> Result<Option<f64>, Error> {
        self.dev
            .gain_element(self.direction, self.channel, &self.name)
    }

    /// Set gain element value.
    pub fn set(&self, gain: f64) -> Result<(), Error> {
        self.dev
            .set_gain_element(self.direction, self.channel, &self.name, gain)
    }

    /// Gain element range.
    pub fn range(&self) -> Result<Range, Error> {
        self.dev
            .gain_element_range(self.direction, self.channel, &self.name)
    }
}

/// Frequency control handle for one channel.
pub struct Frequency<'a, T: FrequencyControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T> Frequency<'a, T>
where
    T: FrequencyControl + ?Sized,
{
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Overall frequency.
    pub fn value(&self) -> Result<f64, Error> {
        self.dev.frequency(self.direction, self.channel)
    }

    /// Set overall frequency.
    pub fn set(&self, frequency: f64) -> Result<(), Error> {
        self.set_with_args(frequency, Args::new())
    }

    /// Set overall frequency with driver arguments.
    pub fn set_with_args(&self, frequency: f64, args: Args) -> Result<(), Error> {
        self.dev
            .set_frequency(self.direction, self.channel, frequency, args)
    }

    /// Overall frequency range.
    pub fn range(&self) -> Result<Range, Error> {
        self.dev.frequency_range(self.direction, self.channel)
    }

    /// Named frequency components.
    pub fn components(&self) -> Result<Vec<String>, Error> {
        self.dev.frequency_components(self.direction, self.channel)
    }

    /// Named frequency component handle.
    pub fn component(&self, name: &str) -> FrequencyComponent<'a, T> {
        FrequencyComponent {
            dev: self.dev,
            direction: self.direction,
            channel: self.channel,
            name: name.to_string(),
        }
    }
}

/// Frequency component handle for one channel.
pub struct FrequencyComponent<'a, T: FrequencyControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
    name: String,
}

impl<'a, T> FrequencyComponent<'a, T>
where
    T: FrequencyControl + ?Sized,
{
    /// Frequency component value.
    pub fn value(&self) -> Result<f64, Error> {
        self.dev
            .component_frequency(self.direction, self.channel, &self.name)
    }

    /// Set frequency component value.
    pub fn set(&self, frequency: f64) -> Result<(), Error> {
        self.dev
            .set_component_frequency(self.direction, self.channel, &self.name, frequency)
    }

    /// Frequency component range.
    pub fn range(&self) -> Result<Range, Error> {
        self.dev
            .component_frequency_range(self.direction, self.channel, &self.name)
    }
}

/// Sample-rate control handle for one channel.
pub struct SampleRate<'a, T: SampleRateControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T> SampleRate<'a, T>
where
    T: SampleRateControl + ?Sized,
{
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Sample rate.
    pub fn value(&self) -> Result<f64, Error> {
        self.dev.sample_rate(self.direction, self.channel)
    }

    /// Set sample rate.
    pub fn set(&self, rate: f64) -> Result<(), Error> {
        self.dev.set_sample_rate(self.direction, self.channel, rate)
    }

    /// Sample-rate range.
    pub fn range(&self) -> Result<Range, Error> {
        self.dev.get_sample_rate_range(self.direction, self.channel)
    }
}

/// Bandwidth control handle for one channel.
pub struct Bandwidth<'a, T: BandwidthControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T> Bandwidth<'a, T>
where
    T: BandwidthControl + ?Sized,
{
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Bandwidth.
    pub fn value(&self) -> Result<f64, Error> {
        self.dev.bandwidth(self.direction, self.channel)
    }

    /// Set bandwidth.
    pub fn set(&self, bandwidth: f64) -> Result<(), Error> {
        self.dev
            .set_bandwidth(self.direction, self.channel, bandwidth)
    }

    /// Bandwidth range.
    pub fn range(&self) -> Result<Range, Error> {
        self.dev.get_bandwidth_range(self.direction, self.channel)
    }
}

/// Automatic DC offset correction handle for one channel.
pub struct DcOffset<'a, T: DcOffsetControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T> DcOffset<'a, T>
where
    T: DcOffsetControl + ?Sized,
{
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Result<Self, Error> {
        if !dev.dc_offset_available(direction, channel)? {
            return Err(Error::unsupported(Capability::DcOffset));
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

    /// Borrow the underlying device implementation.
    pub fn as_inner(&self) -> &T {
        &self.dev
    }

    /// Mutably borrow the underlying device implementation.
    pub fn as_inner_mut(&mut self) -> &mut T {
        &mut self.dev
    }

    /// Consume this device and return the underlying implementation.
    pub fn into_inner(self) -> T {
        self.dev
    }
}

impl<T> Device<T>
where
    T: TypedDeviceBackend,
{
    /// Open a typed device matching `args`.
    pub fn from_args<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        match args.get::<Driver>("driver") {
            Ok(driver) if driver != <T as TypedDeviceBackend>::driver() => {
                return Err(Error::DriverMismatch {
                    expected: <T as TypedDeviceBackend>::driver(),
                    requested: driver,
                });
            }
            Ok(_) | Err(Error::MissingArgument { .. }) => {}
            Err(e) => return Err(e),
        }
        Ok(Self::from_impl(T::open(&args)?))
    }
}

impl<T> Device<T>
where
    T: DynDeviceBackend + 'static,
{
    /// Convert this typed device into a runtime-dispatched device.
    pub fn erase(self) -> DynDevice {
        DynDevice::from_impl(self.dev)
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

    /// Borrow the underlying device implementation as type `D`.
    pub fn impl_ref<D: DeviceInfo + 'static>(&self) -> Result<&D, Error> {
        self.dev
            .as_any()
            .downcast_ref::<D>()
            .ok_or_else(|| Error::invalid_argument("type", "device implementation type mismatch"))
    }

    /// Mutably borrow the underlying device implementation as type `D`.
    pub fn impl_mut<D: DeviceInfo + 'static>(&mut self) -> Result<&mut D, Error> {
        self.dev
            .as_any_mut()
            .downcast_mut::<D>()
            .ok_or_else(|| Error::invalid_argument("type", "device implementation type mismatch"))
    }
}

impl DynDevice {
    /// Open the first discovered runtime-dispatched device.
    pub fn new() -> Result<Self, Error> {
        let registry = Registry::default();
        let descriptors = registry.probe(Args::new())?;
        let descriptor = descriptors.first().ok_or(Error::DeviceNotFound)?;
        registry.open(descriptor)
    }

    /// Open a runtime-dispatched device matching `args`.
    pub fn from_args<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        Registry::default().open_args(args)
    }

    /// Create a runtime-dispatched device from a concrete implementation.
    pub fn from_impl<T: DynDeviceBackend + 'static>(dev: T) -> Self {
        Self {
            inner: Arc::new(dev),
        }
    }

    /// Borrow the erased backend.
    pub fn as_backend(&self) -> &dyn DynDeviceBackend {
        self.inner.as_ref()
    }

    /// Try to downcast to a concrete device implementation.
    pub fn downcast_ref<D: DeviceInfo + 'static>(&self) -> Option<&D> {
        self.inner.as_any().downcast_ref::<D>()
    }

    /// Try to downcast mutably to a concrete device implementation.
    pub fn downcast_mut<D: DeviceInfo + 'static>(&mut self) -> Option<&mut D> {
        Arc::get_mut(&mut self.inner)?
            .as_any_mut()
            .downcast_mut::<D>()
    }

    /// SDR [driver](Driver).
    pub fn driver(&self) -> Driver {
        self.inner.driver()
    }

    /// Identifier for the device, e.g. its serial.
    pub fn id(&self) -> Result<String, Error> {
        self.inner.id()
    }

    /// Device info that can be displayed to the user.
    pub fn info(&self) -> Result<Args, Error> {
        self.inner.info()
    }

    /// Structured runtime capabilities for the device.
    pub fn capabilities(&self) -> Result<DeviceCapabilities, Error> {
        self.inner.capabilities()
    }

    /// RX channel handle.
    pub fn rx(&self, index: usize) -> Result<RxChannel<'_, Self>, Error> {
        ensure_channel(self, Direction::Rx, index)?;
        Ok(RxChannel::new(self, index))
    }

    /// TX channel handle.
    pub fn tx(&self, index: usize) -> Result<TxChannel<'_, Self>, Error> {
        ensure_channel(self, Direction::Tx, index)?;
        Ok(TxChannel::new(self, index))
    }

    /// Create an RX streamer.
    pub fn rx_streamer(&self, channels: &[usize]) -> Result<DynRxStreamer, Error> {
        self.rx_streamer_with_args(channels, Args::new())
    }

    /// Create an RX streamer, using `args`.
    pub fn rx_streamer_with_args<A: TryInto<Args>>(
        &self,
        channels: &[usize],
        args: A,
    ) -> Result<DynRxStreamer, Error> {
        for channel in channels {
            ensure_channel(self, Direction::Rx, *channel)?;
        }
        <Self as RxDevice>::rx_streamer(
            self,
            channels,
            args.try_into()
                .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?,
        )
    }

    /// Create a TX streamer.
    pub fn tx_streamer(&self, channels: &[usize]) -> Result<DynTxStreamer, Error> {
        self.tx_streamer_with_args(channels, Args::new())
    }

    /// Create a TX streamer, using `args`.
    pub fn tx_streamer_with_args<A: TryInto<Args>>(
        &self,
        channels: &[usize],
        args: A,
    ) -> Result<DynTxStreamer, Error> {
        for channel in channels {
            ensure_channel(self, Direction::Tx, *channel)?;
        }
        <Self as TxDevice>::tx_streamer(
            self,
            channels,
            args.try_into()
                .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?,
        )
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
        self.inner.driver()
    }
    fn id(&self) -> Result<String, Error> {
        self.inner.id()
    }
    fn info(&self) -> Result<Args, Error> {
        self.inner.info()
    }
}

impl ChannelInfo for DynDevice {
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        self.inner
            .as_ref()
            .channel_info()
            .ok_or_else(|| Error::unsupported(Capability::ChannelInfo))?
            .num_channels(direction)
    }
    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.inner
            .as_ref()
            .channel_info()
            .ok_or_else(|| Error::unsupported(Capability::ChannelInfo))?
            .full_duplex(direction, channel)
    }
}

impl RxDevice for DynDevice {
    type RxStreamer = DynRxStreamer;

    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error> {
        self.inner
            .as_ref()
            .rx_device()
            .ok_or_else(|| Error::unsupported(Capability::RxStreaming))?
            .rx_streamer(channels, args)
    }
}

impl TxDevice for DynDevice {
    type TxStreamer = DynTxStreamer;

    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error> {
        self.inner
            .as_ref()
            .tx_device()
            .ok_or_else(|| Error::unsupported(Capability::TxStreaming))?
            .tx_streamer(channels, args)
    }
}

impl AntennaControl for DynDevice {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.inner
            .as_ref()
            .antenna_control()
            .ok_or_else(|| Error::unsupported(Capability::Antenna))?
            .antennas(direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.inner
            .as_ref()
            .antenna_control()
            .ok_or_else(|| Error::unsupported(Capability::Antenna))?
            .antenna(direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        self.inner
            .as_ref()
            .antenna_control()
            .ok_or_else(|| Error::unsupported(Capability::Antenna))?
            .set_antenna(direction, channel, name)
    }
}

impl GainControl for DynDevice {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.inner
            .as_ref()
            .gain_control()
            .ok_or_else(|| Error::unsupported(Capability::Gain))?
            .gain_elements(direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.inner
            .as_ref()
            .gain_control()
            .ok_or_else(|| Error::unsupported(Capability::Gain))?
            .set_gain(direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.inner
            .as_ref()
            .gain_control()
            .ok_or_else(|| Error::unsupported(Capability::Gain))?
            .gain(direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.inner
            .as_ref()
            .gain_control()
            .ok_or_else(|| Error::unsupported(Capability::Gain))?
            .gain_range(direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        self.inner
            .as_ref()
            .gain_control()
            .ok_or_else(|| Error::unsupported(Capability::Gain))?
            .set_gain_element(direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        self.inner
            .as_ref()
            .gain_control()
            .ok_or_else(|| Error::unsupported(Capability::Gain))?
            .gain_element(direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.inner
            .as_ref()
            .gain_control()
            .ok_or_else(|| Error::unsupported(Capability::Gain))?
            .gain_element_range(direction, channel, name)
    }
}

impl FrequencyControl for DynDevice {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.inner
            .as_ref()
            .frequency_control()
            .ok_or_else(|| Error::unsupported(Capability::Frequency))?
            .frequency_range(direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.inner
            .as_ref()
            .frequency_control()
            .ok_or_else(|| Error::unsupported(Capability::Frequency))?
            .frequency(direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        self.inner
            .as_ref()
            .frequency_control()
            .ok_or_else(|| Error::unsupported(Capability::Frequency))?
            .set_frequency(direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.inner
            .as_ref()
            .frequency_control()
            .ok_or_else(|| Error::unsupported(Capability::Frequency))?
            .frequency_components(direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.inner
            .as_ref()
            .frequency_control()
            .ok_or_else(|| Error::unsupported(Capability::Frequency))?
            .component_frequency_range(direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.inner
            .as_ref()
            .frequency_control()
            .ok_or_else(|| Error::unsupported(Capability::Frequency))?
            .component_frequency(direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        self.inner
            .as_ref()
            .frequency_control()
            .ok_or_else(|| Error::unsupported(Capability::Frequency))?
            .set_component_frequency(direction, channel, name, frequency)
    }
}

impl SampleRateControl for DynDevice {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.inner
            .as_ref()
            .sample_rate_control()
            .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
            .sample_rate(direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.inner
            .as_ref()
            .sample_rate_control()
            .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
            .set_sample_rate(direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.inner
            .as_ref()
            .sample_rate_control()
            .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
            .get_sample_rate_range(direction, channel)
    }
}

impl BandwidthControl for DynDevice {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.inner
            .as_ref()
            .bandwidth_control()
            .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
            .bandwidth(direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        self.inner
            .as_ref()
            .bandwidth_control()
            .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
            .set_bandwidth(direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.inner
            .as_ref()
            .bandwidth_control()
            .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
            .get_bandwidth_range(direction, channel)
    }
}

impl AgcControl for DynDevice {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.inner
            .as_ref()
            .agc_control()
            .ok_or_else(|| Error::unsupported(Capability::Agc))?
            .agc_available(direction, channel)
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.inner
            .as_ref()
            .agc_control()
            .ok_or_else(|| Error::unsupported(Capability::Agc))?
            .agc_enabled(direction, channel)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        self.inner
            .as_ref()
            .agc_control()
            .ok_or_else(|| Error::unsupported(Capability::Agc))?
            .set_agc_enabled(direction, channel, enabled)
    }
}

impl DcOffsetControl for DynDevice {
    fn dc_offset_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.inner
            .as_ref()
            .dc_offset_control()
            .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
            .dc_offset_available(direction, channel)
    }

    fn dc_offset_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.inner
            .as_ref()
            .dc_offset_control()
            .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
            .dc_offset_enabled(direction, channel)
    }

    fn set_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        self.inner
            .as_ref()
            .dc_offset_control()
            .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
            .set_dc_offset_enabled(direction, channel, enabled)
    }
}

impl<T: ChannelInfo> Device<T> {
    /// RX channel handle.
    pub fn rx(&self, index: usize) -> Result<RxChannel<'_, T>, Error> {
        ensure_channel(&self.dev, Direction::Rx, index)?;
        Ok(RxChannel::new(&self.dev, index))
    }

    /// TX channel handle.
    pub fn tx(&self, index: usize) -> Result<TxChannel<'_, T>, Error> {
        ensure_channel(&self.dev, Direction::Tx, index)?;
        Ok(TxChannel::new(&self.dev, index))
    }
}

fn ensure_channel<T>(dev: &T, direction: Direction, channel: usize) -> Result<(), Error>
where
    T: ChannelInfo + ?Sized,
{
    let available = dev.num_channels(direction)?;
    if channel < available {
        Ok(())
    } else {
        Err(Error::invalid_channel(direction, channel, available))
    }
}

impl<T: RxDevice + ChannelInfo> Device<T> {
    /// Create an RX streamer over one or more RX channels.
    pub fn rx_streamer(&self, channels: &[usize]) -> Result<T::RxStreamer, Error> {
        self.rx_streamer_with_args(channels, Args::new())
    }

    /// Create an RX streamer over one or more RX channels, using `args`.
    pub fn rx_streamer_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<T::RxStreamer, Error> {
        for channel in channels {
            ensure_channel(&self.dev, Direction::Rx, *channel)?;
        }
        self.dev.rx_streamer(channels, args)
    }
}

impl<T: TxDevice + ChannelInfo> Device<T> {
    /// Create a TX streamer over one or more TX channels.
    pub fn tx_streamer(&self, channels: &[usize]) -> Result<T::TxStreamer, Error> {
        self.tx_streamer_with_args(channels, Args::new())
    }

    /// Create a TX streamer over one or more TX channels, using `args`.
    pub fn tx_streamer_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<T::TxStreamer, Error> {
        for channel in channels {
            ensure_channel(&self.dev, Direction::Tx, *channel)?;
        }
        self.dev.tx_streamer(channels, args)
    }
}

impl<'a, T: RxDevice + ?Sized> RxChannel<'a, T> {
    /// Create a single-channel RX streamer.
    pub fn streamer(&self) -> Result<T::RxStreamer, Error> {
        self.streamer_with_args(Args::new())
    }

    /// Create a single-channel RX streamer, using `args`.
    pub fn streamer_with_args(&self, args: Args) -> Result<T::RxStreamer, Error> {
        self.dev.rx_streamer(&[self.channel], args)
    }
}

impl<'a, T: TxDevice + ?Sized> TxChannel<'a, T> {
    /// Create a single-channel TX streamer.
    pub fn streamer(&self) -> Result<T::TxStreamer, Error> {
        self.streamer_with_args(Args::new())
    }

    /// Create a single-channel TX streamer, using `args`.
    pub fn streamer_with_args(&self, args: Args) -> Result<T::TxStreamer, Error> {
        self.dev.tx_streamer(&[self.channel], args)
    }
}

impl<'a, T: ChannelInfo + ?Sized> RxChannel<'a, T> {
    /// Full-duplex support for this RX channel.
    pub fn full_duplex(&self) -> Result<bool, Error> {
        self.dev.full_duplex(Direction::Rx, self.channel)
    }
}

impl<'a, T: ChannelInfo + ?Sized> TxChannel<'a, T> {
    /// Full-duplex support for this TX channel.
    pub fn full_duplex(&self) -> Result<bool, Error> {
        self.dev.full_duplex(Direction::Tx, self.channel)
    }
}

macro_rules! impl_channel_controls {
    ($channel:ident, $direction:expr) => {
        impl<'a, T: AntennaControl + ?Sized> $channel<'a, T> {
            /// Antenna control.
            pub fn antenna(&self) -> Result<Antenna<'_, T>, Error> {
                Antenna::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: AgcControl + ?Sized> $channel<'a, T> {
            /// Automatic gain control.
            pub fn agc(&self) -> Result<Agc<'_, T>, Error> {
                Agc::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: GainControl + ?Sized> $channel<'a, T> {
            /// Gain control.
            pub fn gain(&self) -> Result<Gain<'_, T>, Error> {
                Ok(Gain::new(self.dev, $direction, self.channel))
            }
        }

        impl<'a, T: FrequencyControl + ?Sized> $channel<'a, T> {
            /// Frequency control.
            pub fn frequency(&self) -> Result<Frequency<'_, T>, Error> {
                Ok(Frequency::new(self.dev, $direction, self.channel))
            }
        }

        impl<'a, T: SampleRateControl + ?Sized> $channel<'a, T> {
            /// Sample-rate control.
            pub fn sample_rate(&self) -> Result<SampleRate<'_, T>, Error> {
                Ok(SampleRate::new(self.dev, $direction, self.channel))
            }
        }

        impl<'a, T: BandwidthControl + ?Sized> $channel<'a, T> {
            /// Bandwidth control.
            pub fn bandwidth(&self) -> Result<Bandwidth<'_, T>, Error> {
                Ok(Bandwidth::new(self.dev, $direction, self.channel))
            }
        }

        impl<'a, T: DcOffsetControl + ?Sized> $channel<'a, T> {
            /// Automatic DC offset correction.
            pub fn dc_offset(&self) -> Result<DcOffset<'_, T>, Error> {
                DcOffset::new(self.dev, $direction, self.channel)
            }
        }
    };
}

impl_channel_controls!(RxChannel, Direction::Rx);
impl_channel_controls!(TxChannel, Direction::Tx);

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
                _ => Err(Error::invalid_argument(
                    "channels",
                    "unsupported RX channel set",
                )),
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
                Err(Error::invalid_channel(Direction::Rx, channel, 1))
            }
        }

        fn dc_offset_enabled(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
            if channel == 0 {
                Ok(*self.0.lock().unwrap())
            } else {
                Err(Error::invalid_channel(Direction::Rx, channel, 1))
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
                Err(Error::invalid_channel(Direction::Rx, channel, 1))
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
        let dev = DynDevice::from_impl(dummy);

        let capabilities = dev.capabilities().unwrap();

        assert_eq!(capabilities.rx_channels.len(), 1);
        assert_eq!(capabilities.tx_channels.len(), 1);

        let rx0 = &capabilities.rx_channels[0];
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
    fn antenna_handle_reports_ports_and_selected_port() {
        let dummy = crate::impls::Dummy::open(Args::new()).unwrap();
        let dev = Device::from_impl(dummy);
        let rx0 = dev.rx(0).unwrap();
        let antenna = rx0.antenna().unwrap();

        assert_eq!(antenna.ports(), &[String::from("A")]);
        assert_eq!(antenna.selected().unwrap(), "A");
        antenna.select("A").unwrap();
    }

    #[test]
    fn typed_device_erases_to_dyn_device_and_downcasts() {
        let dummy = crate::impls::Dummy::open(Args::new()).unwrap();
        let dev = Device::from_impl(dummy);
        let mut dev = dev.erase();

        assert_eq!(dev.driver(), Driver::Dummy);
        assert!(dev.downcast_ref::<crate::impls::Dummy>().is_some());
        assert!(dev.downcast_mut::<crate::impls::Dummy>().is_some());
    }

    #[test]
    fn agc_handle_controls_enabled_state() {
        let dummy = crate::impls::Dummy::open(Args::new()).unwrap();
        let dev = Device::from_impl(dummy);
        let rx0 = dev.rx(0).unwrap();
        let agc = rx0.agc().unwrap();

        agc.enable().unwrap();
        assert!(agc.enabled().unwrap());

        agc.disable().unwrap();
        assert!(!agc.enabled().unwrap());
    }

    #[test]
    fn dc_offset_handle_controls_enabled_state() {
        let dev = Device::from_impl(DcToggle(std::sync::Mutex::new(false)));
        let rx0 = RxChannel::new(&dev.dev, 0);
        let dc_offset = rx0.dc_offset().unwrap();

        dc_offset.enable().unwrap();
        assert!(dc_offset.enabled().unwrap());

        dc_offset.disable().unwrap();
        assert!(!dc_offset.enabled().unwrap());
    }

    #[test]
    fn dyn_device_does_not_require_all_capabilities() {
        let dev = DynDevice::from_impl(RxOnly);

        let capabilities = dev.capabilities().unwrap();
        assert_eq!(capabilities.rx_channels.len(), 1);
        assert_eq!(capabilities.tx_channels.len(), 0);
        assert_eq!(
            capabilities.rx_channels[0].controls,
            ChannelControls::default()
        );

        assert!(dev.rx_streamer(&[0]).is_ok());
        assert!(matches!(
            dev.tx(0),
            Err(Error::InvalidChannel {
                direction: Direction::Tx,
                channel: 0,
                available: 0
            })
        ));
        let rx0 = dev.rx(0).unwrap();
        assert!(matches!(
            rx0.agc(),
            Err(Error::Unsupported {
                capability: Capability::Agc,
                ..
            })
        ));
        assert!(matches!(
            rx0.dc_offset(),
            Err(Error::Unsupported {
                capability: Capability::DcOffset,
                ..
            })
        ));
    }
}
