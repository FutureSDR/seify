//! Local asynchronous APIs for non-`Send` drivers.
//!
//! This module is intended for runtimes such as browser/WASM environments
//! where futures and driver handles often cannot move between threads.

use futures::future::{FutureExt, LocalBoxFuture};
use std::any::Any;
use std::rc::Rc;

use crate::async_streamer::local;
use crate::Args;
use crate::Capability;
use crate::ChannelCapabilities;
use crate::ChannelControls;
use crate::DeviceCapabilities;
use crate::DeviceDescriptor;
use crate::Direction;
use crate::Driver;
use crate::Error;
use crate::Range;

/// Local asynchronous RX streamer trait.
pub use local::AsyncRxStreamer as RxStreamer;
/// Local asynchronous TX streamer trait.
pub use local::AsyncTxStreamer as TxStreamer;

/// Type-erased local asynchronous RX streamer.
pub type DynRxStreamer = Box<dyn RxStreamer>;

/// Type-erased local asynchronous TX streamer.
pub type DynTxStreamer = Box<dyn TxStreamer>;

/// Object-safe local asynchronous RX streaming capability.
pub trait ErasedRxDevice {
    /// Create a type-erased local asynchronous RX streamer.
    fn rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<DynRxStreamer, Error>>;
}

impl<T> ErasedRxDevice for T
where
    T: RxDevice,
    T::RxStreamer: 'static,
{
    fn rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<DynRxStreamer, Error>> {
        async move { Ok(Box::new(RxDevice::rx_streamer(self, channels, args).await?) as DynRxStreamer) }
            .boxed_local()
    }
}

/// Object-safe local asynchronous TX streaming capability.
pub trait ErasedTxDevice {
    /// Create a type-erased local asynchronous TX streamer.
    fn tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<DynTxStreamer, Error>>;
}

impl<T> ErasedTxDevice for T
where
    T: TxDevice,
    T::TxStreamer: 'static,
{
    fn tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<DynTxStreamer, Error>> {
        async move { Ok(Box::new(TxDevice::tx_streamer(self, channels, args).await?) as DynTxStreamer) }
            .boxed_local()
    }
}

/// Runtime-dispatched local asynchronous device backend.
pub trait DynDeviceBackend: DeviceInfo {
    /// Return a structured snapshot of the device's runtime capabilities.
    fn capabilities(&self) -> LocalBoxFuture<'_, Result<DeviceCapabilities, Error>> {
        async { device_capabilities(self).await }.boxed_local()
    }

    /// Return channel metadata capability, if exposed.
    fn channel_info(&self) -> Option<&dyn ChannelInfo> {
        None
    }

    /// Return RX streaming capability, if exposed.
    fn rx_device(&self) -> Option<&dyn ErasedRxDevice> {
        None
    }

    /// Return TX streaming capability, if exposed.
    fn tx_device(&self) -> Option<&dyn ErasedTxDevice> {
        None
    }

    /// Return antenna control capability, if exposed.
    fn antenna_control(&self) -> Option<&dyn AntennaControl> {
        None
    }

    /// Return automatic gain control capability, if exposed.
    fn agc_control(&self) -> Option<&dyn AgcControl> {
        None
    }

    /// Return gain control capability, if exposed.
    fn gain_control(&self) -> Option<&dyn GainControl> {
        None
    }

    /// Return frequency control capability, if exposed.
    fn frequency_control(&self) -> Option<&dyn FrequencyControl> {
        None
    }

    /// Return sample-rate control capability, if exposed.
    fn sample_rate_control(&self) -> Option<&dyn SampleRateControl> {
        None
    }

    /// Return bandwidth control capability, if exposed.
    fn bandwidth_control(&self) -> Option<&dyn BandwidthControl> {
        None
    }

    /// Return DC offset control capability, if exposed.
    fn dc_offset_control(&self) -> Option<&dyn DcOffsetControl> {
        None
    }
}

/// Runtime-dispatched local asynchronous opened device.
#[derive(Clone)]
pub struct DynDevice {
    inner: Rc<dyn DynDeviceBackend>,
}

/// Basic local asynchronous device metadata.
pub trait DeviceInfo {
    /// Cast to [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Cast to [`Any`] for mutable downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// SDR driver.
    fn driver(&self) -> Driver;
    /// Identifier for the device, e.g. its serial.
    fn id(&self) -> LocalBoxFuture<'_, Result<String, Error>>;
    /// Device info that can be displayed to the user.
    fn info(&self) -> LocalBoxFuture<'_, Result<Args, Error>>;
}

/// Basic local asynchronous channel metadata.
pub trait ChannelInfo {
    /// Number of supported channels.
    fn num_channels(&self, direction: Direction) -> LocalBoxFuture<'_, Result<usize, Error>>;
    /// Full-duplex support.
    fn full_duplex(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>>;
}

/// Local asynchronous RX streaming capability.
pub trait RxDevice {
    /// RX streamer implementation.
    type RxStreamer: RxStreamer;

    /// Create an RX streamer.
    fn rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<Self::RxStreamer, Error>>;
}

/// Local asynchronous TX streaming capability.
pub trait TxDevice {
    /// TX streamer implementation.
    type TxStreamer: TxStreamer;

    /// Create a TX streamer.
    fn tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<Self::TxStreamer, Error>>;
}

/// Local asynchronous antenna control capability.
pub trait AntennaControl {
    /// Return available antenna port names.
    fn antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Vec<String>, Error>>;
    /// Return the selected antenna port name.
    fn antenna(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<String, Error>>;
    /// Select an antenna port by name.
    fn set_antenna<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> LocalBoxFuture<'a, Result<(), Error>>;
}

/// Local asynchronous automatic gain control capability.
pub trait AgcControl {
    /// Return whether automatic gain control is available.
    fn agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>>;
    /// Return whether automatic gain control is enabled.
    fn agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>>;
    /// Enable or disable automatic gain control.
    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> LocalBoxFuture<'_, Result<(), Error>>;
}

/// Local asynchronous gain control capability.
pub trait GainControl {
    /// Return named gain elements available for the channel.
    fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Vec<String>, Error>>;
    /// Set overall channel gain in dB.
    fn set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> LocalBoxFuture<'_, Result<(), Error>>;
    /// Return overall channel gain in dB, if available.
    fn gain(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Option<f64>, Error>>;
    /// Return supported overall channel gain range in dB.
    fn gain_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Range, Error>>;
}

/// Local asynchronous frequency control capability.
pub trait FrequencyControl {
    /// Return supported overall tuning range in Hz.
    fn frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Range, Error>>;
    /// Return current overall channel frequency in Hz.
    fn frequency(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<f64, Error>>;
    /// Set overall channel frequency in Hz with optional driver arguments.
    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> LocalBoxFuture<'_, Result<(), Error>>;
    /// Return named frequency components for the channel.
    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Vec<String>, Error>>;
}

/// Local asynchronous sample-rate control capability.
pub trait SampleRateControl {
    /// Return current sample rate in samples per second.
    fn sample_rate(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<f64, Error>>;
    /// Set sample rate in samples per second.
    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> LocalBoxFuture<'_, Result<(), Error>>;
    /// Return supported sample-rate range in samples per second.
    fn sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Range, Error>>;
}

/// Local asynchronous bandwidth control capability.
pub trait BandwidthControl {
    /// Return current channel bandwidth in Hz.
    fn bandwidth(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<f64, Error>>;
    /// Set channel bandwidth in Hz.
    fn set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bandwidth: f64,
    ) -> LocalBoxFuture<'_, Result<(), Error>>;
    /// Return supported channel bandwidth range in Hz.
    fn bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Range, Error>>;
}

/// Local asynchronous automatic DC offset correction capability.
pub trait DcOffsetControl {
    /// Return whether automatic DC offset correction is available.
    fn dc_offset_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>>;
    /// Return whether automatic DC offset correction is enabled.
    fn dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>>;
    /// Enable or disable automatic DC offset correction.
    fn set_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> LocalBoxFuture<'_, Result<(), Error>>;
}

async fn device_capabilities<D>(dev: &D) -> Result<DeviceCapabilities, Error>
where
    D: DynDeviceBackend + ?Sized,
{
    Ok(DeviceCapabilities {
        rx_channels: channel_capabilities(dev, Direction::Rx).await?,
        tx_channels: channel_capabilities(dev, Direction::Tx).await?,
    })
}

async fn channel_capabilities<D>(
    dev: &D,
    direction: Direction,
) -> Result<Vec<ChannelCapabilities>, Error>
where
    D: DynDeviceBackend + ?Sized,
{
    let Some(channel_info) = dev.channel_info() else {
        return Ok(Vec::new());
    };
    let channels = match channel_info.num_channels(direction).await {
        Ok(channels) => channels,
        Err(e) if e.is_unsupported() => 0,
        Err(e) => return Err(e),
    };

    let mut out = Vec::with_capacity(channels);
    for channel in 0..channels {
        out.push(ChannelCapabilities {
            channel,
            full_duplex: optional_capability(channel_info.full_duplex(direction, channel).await)?,
            controls: ChannelControls {
                antennas: optional_capability_async(dev.antenna_control(), |cap| {
                    cap.antennas(direction, channel)
                })
                .await?,
                agc: capability_available_async(dev.agc_control(), |cap| {
                    cap.agc_available(direction, channel)
                })
                .await?,
                gain_elements: optional_capability_async(dev.gain_control(), |cap| {
                    cap.gain_elements(direction, channel)
                })
                .await?,
                gain_range: optional_capability_async(dev.gain_control(), |cap| {
                    cap.gain_range(direction, channel)
                })
                .await?,
                frequency_components: optional_capability_async(dev.frequency_control(), |cap| {
                    cap.frequency_components(direction, channel)
                })
                .await?,
                frequency_range: optional_capability_async(dev.frequency_control(), |cap| {
                    cap.frequency_range(direction, channel)
                })
                .await?,
                sample_rate_range: optional_capability_async(dev.sample_rate_control(), |cap| {
                    cap.sample_rate_range(direction, channel)
                })
                .await?,
                bandwidth_range: optional_capability_async(dev.bandwidth_control(), |cap| {
                    cap.bandwidth_range(direction, channel)
                })
                .await?,
                dc_offset: capability_available_async(dev.dc_offset_control(), |cap| {
                    cap.dc_offset_available(direction, channel)
                })
                .await?,
            },
        });
    }

    Ok(out)
}

fn optional_capability<T>(result: Result<T, Error>) -> Result<Option<T>, Error> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.is_unsupported() => Ok(None),
        Err(e) => Err(e),
    }
}

async fn optional_capability_async<C: ?Sized, T>(
    cap: Option<&C>,
    f: impl for<'a> FnOnce(&'a C) -> LocalBoxFuture<'a, Result<T, Error>>,
) -> Result<Option<T>, Error> {
    match cap {
        Some(cap) => optional_capability(f(cap).await),
        None => Ok(None),
    }
}

async fn capability_available_async<C: ?Sized>(
    cap: Option<&C>,
    f: impl for<'a> FnOnce(&'a C) -> LocalBoxFuture<'a, Result<bool, Error>>,
) -> Result<bool, Error> {
    match cap {
        Some(cap) => match f(cap).await {
            Ok(available) => Ok(available),
            Err(e) if e.is_unsupported() => Ok(false),
            Err(e) => Err(e),
        },
        None => Ok(false),
    }
}

/// RX channel handle for local asynchronous devices.
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

/// TX channel handle for local asynchronous devices.
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

/// Local asynchronous antenna control handle for one channel.
pub struct Antenna<'a, T: AntennaControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AntennaControl + ?Sized> Antenna<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Selectable antenna ports.
    pub fn ports(&self) -> LocalBoxFuture<'_, Result<Vec<String>, Error>> {
        self.dev.antennas(self.direction, self.channel)
    }

    /// Currently selected antenna.
    pub fn selected(&self) -> LocalBoxFuture<'_, Result<String, Error>> {
        self.dev.antenna(self.direction, self.channel)
    }

    /// Select an antenna port.
    pub fn select<'b>(&'b self, name: &'b str) -> LocalBoxFuture<'b, Result<(), Error>> {
        self.dev.set_antenna(self.direction, self.channel, name)
    }
}

/// Local asynchronous automatic gain control handle for one channel.
pub struct Agc<'a, T: AgcControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AgcControl + ?Sized> Agc<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Return whether automatic gain control is enabled.
    pub fn enabled(&self) -> LocalBoxFuture<'_, Result<bool, Error>> {
        async move {
            if !self.dev.agc_available(self.direction, self.channel).await? {
                return Err(Error::unsupported(Capability::Agc));
            }
            self.dev.agc_enabled(self.direction, self.channel).await
        }
        .boxed_local()
    }

    /// Enable automatic gain control.
    pub fn enable(&self) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.set_enabled(true)
    }

    /// Disable automatic gain control.
    pub fn disable(&self) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.set_enabled(false)
    }

    /// Set whether automatic gain control is enabled.
    pub fn set_enabled(&self, enabled: bool) -> LocalBoxFuture<'_, Result<(), Error>> {
        async move {
            if !self.dev.agc_available(self.direction, self.channel).await? {
                return Err(Error::unsupported(Capability::Agc));
            }
            self.dev
                .set_agc_enabled(self.direction, self.channel, enabled)
                .await
        }
        .boxed_local()
    }
}

/// Local asynchronous gain control handle for one channel.
pub struct Gain<'a, T: GainControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: GainControl + ?Sized> Gain<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Named gain elements.
    pub fn elements(&self) -> LocalBoxFuture<'_, Result<Vec<String>, Error>> {
        self.dev.gain_elements(self.direction, self.channel)
    }

    /// Overall gain.
    pub fn value(&self) -> LocalBoxFuture<'_, Result<Option<f64>, Error>> {
        self.dev.gain(self.direction, self.channel)
    }

    /// Set overall gain.
    pub fn set(&self, gain: f64) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.dev.set_gain(self.direction, self.channel, gain)
    }

    /// Overall gain range.
    pub fn range(&self) -> LocalBoxFuture<'_, Result<Range, Error>> {
        self.dev.gain_range(self.direction, self.channel)
    }
}

/// Local asynchronous frequency control handle for one channel.
pub struct Frequency<'a, T: FrequencyControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: FrequencyControl + ?Sized> Frequency<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Overall frequency.
    pub fn value(&self) -> LocalBoxFuture<'_, Result<f64, Error>> {
        self.dev.frequency(self.direction, self.channel)
    }

    /// Set overall frequency.
    pub fn set(&self, frequency: f64) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.set_with_args(frequency, Args::new())
    }

    /// Set overall frequency with driver arguments.
    pub fn set_with_args(
        &self,
        frequency: f64,
        args: Args,
    ) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.dev
            .set_frequency(self.direction, self.channel, frequency, args)
    }

    /// Overall frequency range.
    pub fn range(&self) -> LocalBoxFuture<'_, Result<Range, Error>> {
        self.dev.frequency_range(self.direction, self.channel)
    }

    /// Named frequency components.
    pub fn components(&self) -> LocalBoxFuture<'_, Result<Vec<String>, Error>> {
        self.dev.frequency_components(self.direction, self.channel)
    }
}

/// Local asynchronous sample-rate control handle for one channel.
pub struct SampleRate<'a, T: SampleRateControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: SampleRateControl + ?Sized> SampleRate<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Sample rate.
    pub fn value(&self) -> LocalBoxFuture<'_, Result<f64, Error>> {
        self.dev.sample_rate(self.direction, self.channel)
    }

    /// Set sample rate.
    pub fn set(&self, rate: f64) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.dev.set_sample_rate(self.direction, self.channel, rate)
    }

    /// Sample-rate range.
    pub fn range(&self) -> LocalBoxFuture<'_, Result<Range, Error>> {
        self.dev.sample_rate_range(self.direction, self.channel)
    }
}

/// Local asynchronous bandwidth control handle for one channel.
pub struct Bandwidth<'a, T: BandwidthControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: BandwidthControl + ?Sized> Bandwidth<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Bandwidth.
    pub fn value(&self) -> LocalBoxFuture<'_, Result<f64, Error>> {
        self.dev.bandwidth(self.direction, self.channel)
    }

    /// Set bandwidth.
    pub fn set(&self, bandwidth: f64) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.dev
            .set_bandwidth(self.direction, self.channel, bandwidth)
    }

    /// Bandwidth range.
    pub fn range(&self) -> LocalBoxFuture<'_, Result<Range, Error>> {
        self.dev.bandwidth_range(self.direction, self.channel)
    }
}

/// Local asynchronous automatic DC offset correction handle for one channel.
pub struct DcOffset<'a, T: DcOffsetControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: DcOffsetControl + ?Sized> DcOffset<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Return whether automatic DC offset correction is enabled.
    pub fn enabled(&self) -> LocalBoxFuture<'_, Result<bool, Error>> {
        async move {
            if !self
                .dev
                .dc_offset_available(self.direction, self.channel)
                .await?
            {
                return Err(Error::unsupported(Capability::DcOffset));
            }
            self.dev
                .dc_offset_enabled(self.direction, self.channel)
                .await
        }
        .boxed_local()
    }

    /// Enable automatic DC offset correction.
    pub fn enable(&self) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.set_enabled(true)
    }

    /// Disable automatic DC offset correction.
    pub fn disable(&self) -> LocalBoxFuture<'_, Result<(), Error>> {
        self.set_enabled(false)
    }

    /// Set whether automatic DC offset correction is enabled.
    pub fn set_enabled(&self, enabled: bool) -> LocalBoxFuture<'_, Result<(), Error>> {
        async move {
            if !self
                .dev
                .dc_offset_available(self.direction, self.channel)
                .await?
            {
                return Err(Error::unsupported(Capability::DcOffset));
            }
            self.dev
                .set_dc_offset_enabled(self.direction, self.channel, enabled)
                .await
        }
        .boxed_local()
    }
}

/// Wraps a local asynchronous driver implementation.
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
    T: DynDeviceBackend + 'static,
{
    /// Convert this typed device into a runtime-dispatched local asynchronous device.
    pub fn erase(self) -> DynDevice {
        DynDevice::from_impl(self.dev)
    }
}

impl<T: DeviceInfo> Device<T> {
    /// SDR driver.
    pub fn driver(&self) -> Driver {
        self.dev.driver()
    }

    /// Identifier for the device, e.g. its serial.
    pub fn id(&self) -> LocalBoxFuture<'_, Result<String, Error>> {
        self.dev.id()
    }

    /// Device info that can be displayed to the user.
    pub fn info(&self) -> LocalBoxFuture<'_, Result<Args, Error>> {
        self.dev.info()
    }
}

impl DynDevice {
    /// Create a runtime-dispatched local asynchronous device from an implementation.
    pub fn from_impl<T: DynDeviceBackend + 'static>(dev: T) -> Self {
        Self {
            inner: Rc::new(dev),
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
        Rc::get_mut(&mut self.inner)?
            .as_any_mut()
            .downcast_mut::<D>()
    }

    /// SDR driver.
    pub fn driver(&self) -> Driver {
        self.inner.driver()
    }

    /// Identifier for the device, e.g. its serial.
    pub fn id(&self) -> LocalBoxFuture<'_, Result<String, Error>> {
        self.inner.id()
    }

    /// Device info that can be displayed to the user.
    pub fn info(&self) -> LocalBoxFuture<'_, Result<Args, Error>> {
        self.inner.info()
    }

    /// Structured runtime capabilities for the device.
    pub fn capabilities(&self) -> LocalBoxFuture<'_, Result<DeviceCapabilities, Error>> {
        self.inner.capabilities()
    }

    /// RX channel handle.
    pub fn rx(&self, index: usize) -> LocalBoxFuture<'_, Result<RxChannel<'_, Self>, Error>> {
        async move {
            ensure_channel(self, Direction::Rx, index).await?;
            Ok(RxChannel::new(self, index))
        }
        .boxed_local()
    }

    /// TX channel handle.
    pub fn tx(&self, index: usize) -> LocalBoxFuture<'_, Result<TxChannel<'_, Self>, Error>> {
        async move {
            ensure_channel(self, Direction::Tx, index).await?;
            Ok(TxChannel::new(self, index))
        }
        .boxed_local()
    }

    /// Create an RX streamer.
    pub fn rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
    ) -> LocalBoxFuture<'a, Result<DynRxStreamer, Error>> {
        self.rx_streamer_with_args(channels, Args::new())
    }

    /// Create an RX streamer, using `args`.
    pub fn rx_streamer_with_args<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<DynRxStreamer, Error>> {
        async move {
            for channel in channels {
                ensure_channel(self, Direction::Rx, *channel).await?;
            }
            <Self as RxDevice>::rx_streamer(self, channels, args).await
        }
        .boxed_local()
    }

    /// Create a TX streamer.
    pub fn tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
    ) -> LocalBoxFuture<'a, Result<DynTxStreamer, Error>> {
        self.tx_streamer_with_args(channels, Args::new())
    }

    /// Create a TX streamer, using `args`.
    pub fn tx_streamer_with_args<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<DynTxStreamer, Error>> {
        async move {
            for channel in channels {
                ensure_channel(self, Direction::Tx, *channel).await?;
            }
            <Self as TxDevice>::tx_streamer(self, channels, args).await
        }
        .boxed_local()
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

    fn id(&self) -> LocalBoxFuture<'_, Result<String, Error>> {
        self.inner.id()
    }

    fn info(&self) -> LocalBoxFuture<'_, Result<Args, Error>> {
        self.inner.info()
    }
}

impl ChannelInfo for DynDevice {
    fn num_channels(&self, direction: Direction) -> LocalBoxFuture<'_, Result<usize, Error>> {
        async move {
            self.inner
                .channel_info()
                .ok_or_else(|| Error::unsupported(Capability::ChannelInfo))?
                .num_channels(direction)
                .await
        }
        .boxed_local()
    }

    fn full_duplex(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .channel_info()
                .ok_or_else(|| Error::unsupported(Capability::ChannelInfo))?
                .full_duplex(direction, channel)
                .await
        }
        .boxed_local()
    }
}

impl RxDevice for DynDevice {
    type RxStreamer = DynRxStreamer;

    fn rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<Self::RxStreamer, Error>> {
        async move {
            self.inner
                .rx_device()
                .ok_or_else(|| Error::unsupported(Capability::RxStreaming))?
                .rx_streamer(channels, args)
                .await
        }
        .boxed_local()
    }
}

impl TxDevice for DynDevice {
    type TxStreamer = DynTxStreamer;

    fn tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<Self::TxStreamer, Error>> {
        async move {
            self.inner
                .tx_device()
                .ok_or_else(|| Error::unsupported(Capability::TxStreaming))?
                .tx_streamer(channels, args)
                .await
        }
        .boxed_local()
    }
}

macro_rules! impl_dyn_control {
    ($trait_name:ident, $accessor:ident, $cap:expr, $($body:item),* $(,)?) => {
        impl $trait_name for DynDevice {
            $($body)*
        }
    };
}

impl_dyn_control!(
    AntennaControl,
    antenna_control,
    Capability::Antenna,
    fn antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Vec<String>, Error>> {
        async move {
            self.inner
                .antenna_control()
                .ok_or_else(|| Error::unsupported(Capability::Antenna))?
                .antennas(direction, channel)
                .await
        }
        .boxed_local()
    },
    fn antenna(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<String, Error>> {
        async move {
            self.inner
                .antenna_control()
                .ok_or_else(|| Error::unsupported(Capability::Antenna))?
                .antenna(direction, channel)
                .await
        }
        .boxed_local()
    },
    fn set_antenna<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> LocalBoxFuture<'a, Result<(), Error>> {
        async move {
            self.inner
                .antenna_control()
                .ok_or_else(|| Error::unsupported(Capability::Antenna))?
                .set_antenna(direction, channel, name)
                .await
        }
        .boxed_local()
    }
);

impl AgcControl for DynDevice {
    fn agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .agc_control()
                .ok_or_else(|| Error::unsupported(Capability::Agc))?
                .agc_available(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .agc_control()
                .ok_or_else(|| Error::unsupported(Capability::Agc))?
                .agc_enabled(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> LocalBoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .agc_control()
                .ok_or_else(|| Error::unsupported(Capability::Agc))?
                .set_agc_enabled(direction, channel, enabled)
                .await
        }
        .boxed_local()
    }
}

impl GainControl for DynDevice {
    fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Vec<String>, Error>> {
        async move {
            self.inner
                .gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .gain_elements(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> LocalBoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .set_gain(direction, channel, gain)
                .await
        }
        .boxed_local()
    }
    fn gain(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Option<f64>, Error>> {
        async move {
            self.inner
                .gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .gain(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn gain_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Range, Error>> {
        async move {
            self.inner
                .gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .gain_range(direction, channel)
                .await
        }
        .boxed_local()
    }
}

impl FrequencyControl for DynDevice {
    fn frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Range, Error>> {
        async move {
            self.inner
                .frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .frequency_range(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn frequency(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<f64, Error>> {
        async move {
            self.inner
                .frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .frequency(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> LocalBoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .set_frequency(direction, channel, frequency, args)
                .await
        }
        .boxed_local()
    }
    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Vec<String>, Error>> {
        async move {
            self.inner
                .frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .frequency_components(direction, channel)
                .await
        }
        .boxed_local()
    }
}

impl SampleRateControl for DynDevice {
    fn sample_rate(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<f64, Error>> {
        async move {
            self.inner
                .sample_rate_control()
                .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
                .sample_rate(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> LocalBoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .sample_rate_control()
                .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
                .set_sample_rate(direction, channel, rate)
                .await
        }
        .boxed_local()
    }
    fn sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Range, Error>> {
        async move {
            self.inner
                .sample_rate_control()
                .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
                .sample_rate_range(direction, channel)
                .await
        }
        .boxed_local()
    }
}

impl BandwidthControl for DynDevice {
    fn bandwidth(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<f64, Error>> {
        async move {
            self.inner
                .bandwidth_control()
                .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
                .bandwidth(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bandwidth: f64,
    ) -> LocalBoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .bandwidth_control()
                .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
                .set_bandwidth(direction, channel, bandwidth)
                .await
        }
        .boxed_local()
    }
    fn bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<Range, Error>> {
        async move {
            self.inner
                .bandwidth_control()
                .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
                .bandwidth_range(direction, channel)
                .await
        }
        .boxed_local()
    }
}

impl DcOffsetControl for DynDevice {
    fn dc_offset_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .dc_offset_control()
                .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
                .dc_offset_available(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> LocalBoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .dc_offset_control()
                .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
                .dc_offset_enabled(direction, channel)
                .await
        }
        .boxed_local()
    }
    fn set_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> LocalBoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .dc_offset_control()
                .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
                .set_dc_offset_enabled(direction, channel, enabled)
                .await
        }
        .boxed_local()
    }
}

impl<T: ChannelInfo> Device<T> {
    /// RX channel handle.
    pub fn rx(&self, index: usize) -> LocalBoxFuture<'_, Result<RxChannel<'_, T>, Error>> {
        async move {
            ensure_channel(&self.dev, Direction::Rx, index).await?;
            Ok(RxChannel::new(&self.dev, index))
        }
        .boxed_local()
    }

    /// TX channel handle.
    pub fn tx(&self, index: usize) -> LocalBoxFuture<'_, Result<TxChannel<'_, T>, Error>> {
        async move {
            ensure_channel(&self.dev, Direction::Tx, index).await?;
            Ok(TxChannel::new(&self.dev, index))
        }
        .boxed_local()
    }
}

async fn ensure_channel<T>(dev: &T, direction: Direction, channel: usize) -> Result<(), Error>
where
    T: ChannelInfo + ?Sized,
{
    let available = dev.num_channels(direction).await?;
    if channel < available {
        Ok(())
    } else {
        Err(Error::invalid_channel(direction, channel, available))
    }
}

impl<T: RxDevice + ChannelInfo> Device<T> {
    /// Create an RX streamer over one or more RX channels.
    pub fn rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
    ) -> LocalBoxFuture<'a, Result<T::RxStreamer, Error>> {
        self.rx_streamer_with_args(channels, Args::new())
    }

    /// Create an RX streamer over one or more RX channels, using `args`.
    pub fn rx_streamer_with_args<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<T::RxStreamer, Error>> {
        async move {
            for channel in channels {
                ensure_channel(&self.dev, Direction::Rx, *channel).await?;
            }
            self.dev.rx_streamer(channels, args).await
        }
        .boxed_local()
    }
}

impl<T: TxDevice + ChannelInfo> Device<T> {
    /// Create a TX streamer over one or more TX channels.
    pub fn tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
    ) -> LocalBoxFuture<'a, Result<T::TxStreamer, Error>> {
        self.tx_streamer_with_args(channels, Args::new())
    }

    /// Create a TX streamer over one or more TX channels, using `args`.
    pub fn tx_streamer_with_args<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> LocalBoxFuture<'a, Result<T::TxStreamer, Error>> {
        async move {
            for channel in channels {
                ensure_channel(&self.dev, Direction::Tx, *channel).await?;
            }
            self.dev.tx_streamer(channels, args).await
        }
        .boxed_local()
    }
}

impl<'a, T: RxDevice + ?Sized> RxChannel<'a, T> {
    /// Create a single-channel RX streamer.
    pub fn streamer(&self) -> LocalBoxFuture<'_, Result<T::RxStreamer, Error>> {
        self.streamer_with_args(Args::new())
    }

    /// Create a single-channel RX streamer, using `args`.
    pub fn streamer_with_args(
        &self,
        args: Args,
    ) -> LocalBoxFuture<'_, Result<T::RxStreamer, Error>> {
        async move {
            let channels = [self.channel];
            self.dev.rx_streamer(&channels, args).await
        }
        .boxed_local()
    }
}

impl<'a, T: TxDevice + ?Sized> TxChannel<'a, T> {
    /// Create a single-channel TX streamer.
    pub fn streamer(&self) -> LocalBoxFuture<'_, Result<T::TxStreamer, Error>> {
        self.streamer_with_args(Args::new())
    }

    /// Create a single-channel TX streamer, using `args`.
    pub fn streamer_with_args(
        &self,
        args: Args,
    ) -> LocalBoxFuture<'_, Result<T::TxStreamer, Error>> {
        async move {
            let channels = [self.channel];
            self.dev.tx_streamer(&channels, args).await
        }
        .boxed_local()
    }
}

impl<'a, T: ChannelInfo + ?Sized> RxChannel<'a, T> {
    /// Full-duplex support for this RX channel.
    pub fn full_duplex(&self) -> LocalBoxFuture<'_, Result<bool, Error>> {
        self.dev.full_duplex(Direction::Rx, self.channel)
    }
}

impl<'a, T: ChannelInfo + ?Sized> TxChannel<'a, T> {
    /// Full-duplex support for this TX channel.
    pub fn full_duplex(&self) -> LocalBoxFuture<'_, Result<bool, Error>> {
        self.dev.full_duplex(Direction::Tx, self.channel)
    }
}

macro_rules! impl_channel_controls {
    ($channel:ident, $direction:expr) => {
        impl<'a, T: AntennaControl + ?Sized> $channel<'a, T> {
            /// Antenna control.
            pub fn antenna(&self) -> Antenna<'_, T> {
                Antenna::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: AgcControl + ?Sized> $channel<'a, T> {
            /// Automatic gain control.
            pub fn agc(&self) -> Agc<'_, T> {
                Agc::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: GainControl + ?Sized> $channel<'a, T> {
            /// Gain control.
            pub fn gain(&self) -> Gain<'_, T> {
                Gain::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: FrequencyControl + ?Sized> $channel<'a, T> {
            /// Frequency control.
            pub fn frequency(&self) -> Frequency<'_, T> {
                Frequency::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: SampleRateControl + ?Sized> $channel<'a, T> {
            /// Sample-rate control.
            pub fn sample_rate(&self) -> SampleRate<'_, T> {
                SampleRate::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: BandwidthControl + ?Sized> $channel<'a, T> {
            /// Bandwidth control.
            pub fn bandwidth(&self) -> Bandwidth<'_, T> {
                Bandwidth::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: DcOffsetControl + ?Sized> $channel<'a, T> {
            /// Automatic DC offset correction.
            pub fn dc_offset(&self) -> DcOffset<'_, T> {
                DcOffset::new(self.dev, $direction, self.channel)
            }
        }
    };
}

impl_channel_controls!(RxChannel, Direction::Rx);
impl_channel_controls!(TxChannel, Direction::Tx);

/// Local asynchronous driver discovery/opening backend.
pub trait DriverBackend {
    /// Driver handled by this backend.
    fn driver(&self) -> Driver;
    /// Probe devices matching `args`.
    fn probe<'a>(
        &'a self,
        args: &'a Args,
    ) -> LocalBoxFuture<'a, Result<Vec<DeviceDescriptor>, Error>>;
    /// Open a previously discovered device descriptor.
    fn open<'a>(
        &'a self,
        descriptor: &'a DeviceDescriptor,
    ) -> LocalBoxFuture<'a, Result<DynDevice, Error>>;
}

/// Typed local asynchronous driver implementation that can be opened directly.
pub trait TypedDeviceBackend: DynDeviceBackend + Sized + 'static {
    /// Driver implemented by this backend.
    fn driver() -> Driver;
    /// Probe devices matching `args`.
    fn probe(args: &Args) -> LocalBoxFuture<'_, Result<Vec<Args>, Error>>;
    /// Open a typed device matching `args`.
    fn open(args: &Args) -> LocalBoxFuture<'_, Result<Self, Error>>;
}

/// Registry of local asynchronous driver discovery/opening backends.
pub struct Registry {
    backends: Vec<Box<dyn DriverBackend>>,
}

impl Registry {
    /// Create an empty local asynchronous registry.
    pub fn empty() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Register a local asynchronous driver backend.
    pub fn register<B>(&mut self, backend: B) -> &mut Self
    where
        B: DriverBackend + 'static,
    {
        self.backends.push(Box::new(backend));
        self
    }

    /// Probe devices matching `args`.
    pub fn probe<'a, A>(
        &'a self,
        args: A,
    ) -> LocalBoxFuture<'a, Result<Vec<DeviceDescriptor>, Error>>
    where
        A: TryInto<Args> + 'a,
    {
        async move {
            let args = args
                .try_into()
                .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
            let driver = requested_driver(&args)?;
            let mut descriptors = Vec::new();

            for backend in &self.backends {
                if driver.is_none() || driver == Some(backend.driver()) {
                    descriptors.append(&mut backend.probe(&args).await?);
                }
            }

            Ok(descriptors)
        }
        .boxed_local()
    }

    /// Open a discovered device descriptor.
    pub fn open<'a>(
        &'a self,
        descriptor: &'a DeviceDescriptor,
    ) -> LocalBoxFuture<'a, Result<DynDevice, Error>> {
        async move {
            for backend in &self.backends {
                if backend.driver() != descriptor.driver() {
                    continue;
                }
                match backend.open(descriptor).await {
                    Ok(device) => return Ok(device),
                    Err(Error::DeviceNotFound) => {}
                    Err(e) => return Err(e),
                }
            }
            Err(Error::DeviceNotFound)
        }
        .boxed_local()
    }

    /// Open the first local asynchronous device matching `args`.
    pub fn open_args<'a, A>(&'a self, args: A) -> LocalBoxFuture<'a, Result<DynDevice, Error>>
    where
        A: TryInto<Args> + 'a,
    {
        async move {
            let args = args
                .try_into()
                .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
            let driver = requested_driver(&args)?;

            if let Some(driver) = driver {
                let descriptor = DeviceDescriptor::new(driver, args);
                return self.open(&descriptor).await;
            }

            for backend in &self.backends {
                let descriptor = DeviceDescriptor::new(backend.driver(), args.clone());
                match backend.open(&descriptor).await {
                    Ok(device) => return Ok(device),
                    Err(Error::DeviceNotFound) => {}
                    Err(e) => return Err(e),
                }
            }

            Err(Error::DeviceNotFound)
        }
        .boxed_local()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::empty()
    }
}

fn requested_driver(args: &Args) -> Result<Option<Driver>, Error> {
    match args.get::<Driver>("driver") {
        Ok(driver) => Ok(Some(driver)),
        Err(Error::MissingArgument { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use num_complex::Complex32;
    use std::cell::RefCell;

    struct LocalOnly {
        rate: RefCell<f64>,
    }

    struct LocalRxStreamer;

    impl DeviceInfo for LocalOnly {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn driver(&self) -> Driver {
            Driver::Dummy
        }

        fn id(&self) -> LocalBoxFuture<'_, Result<String, Error>> {
            async { Ok("local-only".to_string()) }.boxed_local()
        }

        fn info(&self) -> LocalBoxFuture<'_, Result<Args, Error>> {
            async { Ok(Args::new()) }.boxed_local()
        }
    }

    impl DynDeviceBackend for LocalOnly {
        fn channel_info(&self) -> Option<&dyn ChannelInfo> {
            Some(self)
        }

        fn rx_device(&self) -> Option<&dyn ErasedRxDevice> {
            Some(self)
        }

        fn sample_rate_control(&self) -> Option<&dyn SampleRateControl> {
            Some(self)
        }
    }

    impl ChannelInfo for LocalOnly {
        fn num_channels(&self, direction: Direction) -> LocalBoxFuture<'_, Result<usize, Error>> {
            async move {
                Ok(match direction {
                    Direction::Rx => 1,
                    Direction::Tx => 0,
                })
            }
            .boxed_local()
        }

        fn full_duplex(
            &self,
            _direction: Direction,
            _channel: usize,
        ) -> LocalBoxFuture<'_, Result<bool, Error>> {
            async { Ok(false) }.boxed_local()
        }
    }

    impl RxDevice for LocalOnly {
        type RxStreamer = LocalRxStreamer;

        fn rx_streamer<'a>(
            &'a self,
            channels: &'a [usize],
            _args: Args,
        ) -> LocalBoxFuture<'a, Result<Self::RxStreamer, Error>> {
            async move {
                match channels {
                    &[0] => Ok(LocalRxStreamer),
                    _ => Err(Error::invalid_argument("channels", "unsupported channel")),
                }
            }
            .boxed_local()
        }
    }

    impl SampleRateControl for LocalOnly {
        fn sample_rate(
            &self,
            _direction: Direction,
            channel: usize,
        ) -> LocalBoxFuture<'_, Result<f64, Error>> {
            async move {
                if channel == 0 {
                    Ok(*self.rate.borrow())
                } else {
                    Err(Error::invalid_channel(Direction::Rx, channel, 1))
                }
            }
            .boxed_local()
        }

        fn set_sample_rate(
            &self,
            _direction: Direction,
            channel: usize,
            rate: f64,
        ) -> LocalBoxFuture<'_, Result<(), Error>> {
            async move {
                if channel == 0 {
                    *self.rate.borrow_mut() = rate;
                    Ok(())
                } else {
                    Err(Error::invalid_channel(Direction::Rx, channel, 1))
                }
            }
            .boxed_local()
        }

        fn sample_rate_range(
            &self,
            _direction: Direction,
            _channel: usize,
        ) -> LocalBoxFuture<'_, Result<Range, Error>> {
            async { Ok(Range::new(vec![crate::RangeItem::Interval(0.0, f64::MAX)])) }.boxed_local()
        }
    }

    impl RxStreamer for LocalRxStreamer {
        fn mtu(&self) -> LocalBoxFuture<'_, Result<usize, Error>> {
            async { Ok(4) }.boxed_local()
        }

        fn activate_at(&mut self, _time_ns: Option<i64>) -> LocalBoxFuture<'_, Result<(), Error>> {
            async { Ok(()) }.boxed_local()
        }

        fn deactivate_at(
            &mut self,
            _time_ns: Option<i64>,
        ) -> LocalBoxFuture<'_, Result<(), Error>> {
            async { Ok(()) }.boxed_local()
        }

        fn read<'a>(
            &'a mut self,
            buffers: &'a mut [&'a mut [Complex32]],
            _timeout_us: i64,
        ) -> LocalBoxFuture<'a, Result<usize, Error>> {
            async move {
                for buffer in buffers.iter_mut() {
                    buffer.fill(Complex32::new(2.0, 0.0));
                }
                Ok(buffers[0].len())
            }
            .boxed_local()
        }
    }

    #[test]
    fn local_dyn_device_accepts_non_send_backend() {
        block_on(async {
            let dev = DynDevice::from_impl(LocalOnly {
                rate: RefCell::new(0.0),
            });
            let rx0 = dev.rx(0).await.unwrap();

            rx0.sample_rate().set(2.0e6).await.unwrap();
            assert_eq!(rx0.sample_rate().value().await.unwrap(), 2.0e6);

            let mut samples = [Complex32::new(0.0, 0.0); 4];
            let mut rx = rx0.streamer().await.unwrap();
            rx.activate().await.unwrap();
            let n = rx.read(&mut [&mut samples], 100).await.unwrap();
            assert_eq!(n, 4);
            assert!(samples
                .iter()
                .all(|sample| *sample == Complex32::new(2.0, 0.0)));
        });
    }
}
