use futures::future::{BoxFuture, FutureExt};
use std::any::Any;
use std::sync::Arc;

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
use crate::{AsyncRxStreamer, AsyncTxStreamer};

/// Type-erased asynchronous RX streamer.
pub type DynAsyncRxStreamer = Box<dyn AsyncRxStreamer>;

/// Type-erased asynchronous TX streamer.
pub type DynAsyncTxStreamer = Box<dyn AsyncTxStreamer>;

/// Object-safe asynchronous RX streaming capability.
pub trait ErasedAsyncRxDevice: Send + Sync {
    /// Create a type-erased asynchronous RX streamer.
    fn async_rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<DynAsyncRxStreamer, Error>>;
}

impl<T> ErasedAsyncRxDevice for T
where
    T: AsyncRxDevice + Sync,
    T::RxStreamer: 'static,
{
    fn async_rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<DynAsyncRxStreamer, Error>> {
        async move {
            Ok(
                Box::new(AsyncRxDevice::async_rx_streamer(self, channels, args).await?)
                    as DynAsyncRxStreamer,
            )
        }
        .boxed()
    }
}

/// Object-safe asynchronous TX streaming capability.
pub trait ErasedAsyncTxDevice: Send + Sync {
    /// Create a type-erased asynchronous TX streamer.
    fn async_tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<DynAsyncTxStreamer, Error>>;
}

impl<T> ErasedAsyncTxDevice for T
where
    T: AsyncTxDevice + Sync,
    T::TxStreamer: 'static,
{
    fn async_tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<DynAsyncTxStreamer, Error>> {
        async move {
            Ok(
                Box::new(AsyncTxDevice::async_tx_streamer(self, channels, args).await?)
                    as DynAsyncTxStreamer,
            )
        }
        .boxed()
    }
}

/// Runtime-dispatched asynchronous device backend.
pub trait AsyncDynDeviceBackend: AsyncDeviceInfo + Send + Sync {
    /// Return a structured snapshot of the device's runtime capabilities.
    fn async_capabilities(&self) -> BoxFuture<'_, Result<DeviceCapabilities, Error>> {
        async { async_device_capabilities(self).await }.boxed()
    }

    /// Return channel metadata capability, if exposed.
    fn async_channel_info(&self) -> Option<&dyn AsyncChannelInfo> {
        None
    }

    /// Return RX streaming capability, if exposed.
    fn async_rx_device(&self) -> Option<&dyn ErasedAsyncRxDevice> {
        None
    }

    /// Return TX streaming capability, if exposed.
    fn async_tx_device(&self) -> Option<&dyn ErasedAsyncTxDevice> {
        None
    }

    /// Return antenna control capability, if exposed.
    fn async_antenna_control(&self) -> Option<&dyn AsyncAntennaControl> {
        None
    }

    /// Return automatic gain control capability, if exposed.
    fn async_agc_control(&self) -> Option<&dyn AsyncAgcControl> {
        None
    }

    /// Return gain control capability, if exposed.
    fn async_gain_control(&self) -> Option<&dyn AsyncGainControl> {
        None
    }

    /// Return frequency control capability, if exposed.
    fn async_frequency_control(&self) -> Option<&dyn AsyncFrequencyControl> {
        None
    }

    /// Return sample-rate control capability, if exposed.
    fn async_sample_rate_control(&self) -> Option<&dyn AsyncSampleRateControl> {
        None
    }

    /// Return bandwidth control capability, if exposed.
    fn async_bandwidth_control(&self) -> Option<&dyn AsyncBandwidthControl> {
        None
    }

    /// Return DC offset control capability, if exposed.
    fn async_dc_offset_control(&self) -> Option<&dyn AsyncDcOffsetControl> {
        None
    }
}

/// Runtime-dispatched asynchronous opened device.
#[derive(Clone)]
pub struct AsyncDynDevice {
    inner: Arc<dyn AsyncDynDeviceBackend>,
}

/// Basic asynchronous device metadata.
pub trait AsyncDeviceInfo: Send + Sync {
    /// Cast to [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Cast to [`Any`] for mutable downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// SDR driver.
    fn driver(&self) -> Driver;
    /// Identifier for the device, e.g. its serial.
    fn async_id(&self) -> BoxFuture<'_, Result<String, Error>>;
    /// Device info that can be displayed to the user.
    fn async_info(&self) -> BoxFuture<'_, Result<Args, Error>>;
}

/// Basic asynchronous channel metadata.
pub trait AsyncChannelInfo: Send + Sync {
    /// Number of supported channels.
    fn async_num_channels(&self, direction: Direction) -> BoxFuture<'_, Result<usize, Error>>;
    /// Full-duplex support.
    fn async_full_duplex(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>>;
}

/// Asynchronous RX streaming capability.
pub trait AsyncRxDevice: Send + Sync {
    /// RX streamer implementation.
    type RxStreamer: AsyncRxStreamer;

    /// Create an RX streamer.
    fn async_rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<Self::RxStreamer, Error>>;
}

/// Asynchronous TX streaming capability.
pub trait AsyncTxDevice: Send + Sync {
    /// TX streamer implementation.
    type TxStreamer: AsyncTxStreamer;

    /// Create a TX streamer.
    fn async_tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<Self::TxStreamer, Error>>;
}

/// Asynchronous antenna control capability.
pub trait AsyncAntennaControl: Send + Sync {
    /// Return available antenna port names.
    fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Vec<String>, Error>>;
    /// Return the selected antenna port name.
    fn async_antenna(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<String, Error>>;
    /// Select an antenna port by name.
    fn async_set_antenna<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<(), Error>>;
}

/// Asynchronous automatic gain control capability.
pub trait AsyncAgcControl: Send + Sync {
    /// Return whether automatic gain control is available.
    fn async_agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>>;
    /// Return whether automatic gain control is enabled.
    fn async_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>>;
    /// Enable or disable automatic gain control.
    fn async_set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> BoxFuture<'_, Result<(), Error>>;
}

/// Asynchronous gain control capability.
pub trait AsyncGainControl: Send + Sync {
    /// Return named gain elements available for the channel.
    fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Vec<String>, Error>>;
    /// Set overall channel gain in dB.
    fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> BoxFuture<'_, Result<(), Error>>;
    /// Return overall channel gain in dB, if available.
    fn async_gain(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Option<f64>, Error>>;
    /// Return supported overall channel gain range in dB.
    fn async_gain_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Range, Error>>;
    /// Set a named gain element in dB.
    fn async_set_gain_element<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
        gain: f64,
    ) -> BoxFuture<'a, Result<(), Error>>;
    /// Return a named gain element in dB, if available.
    fn async_gain_element<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Option<f64>, Error>>;
    /// Return supported range in dB for a named gain element.
    fn async_gain_element_range<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Range, Error>>;
}

/// Asynchronous frequency control capability.
pub trait AsyncFrequencyControl: Send + Sync {
    /// Return supported overall tuning range in Hz.
    fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Range, Error>>;
    /// Return current overall channel frequency in Hz.
    fn async_frequency(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<f64, Error>>;
    /// Set overall channel frequency in Hz with optional driver arguments.
    fn async_set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> BoxFuture<'_, Result<(), Error>>;
    /// Return named frequency components for the channel.
    fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Vec<String>, Error>>;
    /// Return supported range in Hz for a named frequency component.
    fn async_component_frequency_range<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Range, Error>>;
    /// Return current frequency in Hz for a named frequency component.
    fn async_component_frequency<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<f64, Error>>;
    /// Set frequency in Hz for a named frequency component.
    fn async_set_component_frequency<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
        frequency: f64,
    ) -> BoxFuture<'a, Result<(), Error>>;
}

/// Asynchronous sample-rate control capability.
pub trait AsyncSampleRateControl: Send + Sync {
    /// Return current sample rate in samples per second.
    fn async_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<f64, Error>>;
    /// Set sample rate in samples per second.
    fn async_set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> BoxFuture<'_, Result<(), Error>>;
    /// Return supported sample-rate range in samples per second.
    fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Range, Error>>;
}

/// Asynchronous bandwidth control capability.
pub trait AsyncBandwidthControl: Send + Sync {
    /// Return current channel bandwidth in Hz.
    fn async_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<f64, Error>>;
    /// Set channel bandwidth in Hz.
    fn async_set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bandwidth: f64,
    ) -> BoxFuture<'_, Result<(), Error>>;
    /// Return supported channel bandwidth range in Hz.
    fn async_get_bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Range, Error>>;
}

/// Asynchronous automatic DC offset correction capability.
pub trait AsyncDcOffsetControl: Send + Sync {
    /// Return whether automatic DC offset correction is available.
    fn async_dc_offset_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>>;
    /// Return whether automatic DC offset correction is enabled.
    fn async_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>>;
    /// Enable or disable automatic DC offset correction.
    fn async_set_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> BoxFuture<'_, Result<(), Error>>;
}

async fn async_device_capabilities<D>(dev: &D) -> Result<DeviceCapabilities, Error>
where
    D: AsyncDynDeviceBackend + ?Sized,
{
    Ok(DeviceCapabilities {
        rx_channels: async_channel_capabilities(dev, Direction::Rx).await?,
        tx_channels: async_channel_capabilities(dev, Direction::Tx).await?,
    })
}

async fn async_channel_capabilities<D>(
    dev: &D,
    direction: Direction,
) -> Result<Vec<ChannelCapabilities>, Error>
where
    D: AsyncDynDeviceBackend + ?Sized,
{
    let Some(channel_info) = dev.async_channel_info() else {
        return Ok(Vec::new());
    };
    let channels = match channel_info.async_num_channels(direction).await {
        Ok(channels) => channels,
        Err(e) if e.is_unsupported() => 0,
        Err(e) => return Err(e),
    };

    let mut out = Vec::with_capacity(channels);
    for channel in 0..channels {
        out.push(ChannelCapabilities {
            channel,
            full_duplex: optional_capability(
                channel_info.async_full_duplex(direction, channel).await,
            )?,
            controls: ChannelControls {
                antennas: optional_capability_async(dev.async_antenna_control(), |cap| {
                    cap.async_antennas(direction, channel)
                })
                .await?,
                agc: capability_available_async(dev.async_agc_control(), |cap| {
                    cap.async_agc_available(direction, channel)
                })
                .await?,
                gain_elements: optional_capability_async(dev.async_gain_control(), |cap| {
                    cap.async_gain_elements(direction, channel)
                })
                .await?,
                gain_range: optional_capability_async(dev.async_gain_control(), |cap| {
                    cap.async_gain_range(direction, channel)
                })
                .await?,
                frequency_components: optional_capability_async(
                    dev.async_frequency_control(),
                    |cap| cap.async_frequency_components(direction, channel),
                )
                .await?,
                frequency_range: optional_capability_async(dev.async_frequency_control(), |cap| {
                    cap.async_frequency_range(direction, channel)
                })
                .await?,
                sample_rate_range: optional_capability_async(
                    dev.async_sample_rate_control(),
                    |cap| cap.async_get_sample_rate_range(direction, channel),
                )
                .await?,
                bandwidth_range: optional_capability_async(dev.async_bandwidth_control(), |cap| {
                    cap.async_get_bandwidth_range(direction, channel)
                })
                .await?,
                dc_offset: capability_available_async(dev.async_dc_offset_control(), |cap| {
                    cap.async_dc_offset_available(direction, channel)
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
    f: impl for<'a> FnOnce(&'a C) -> BoxFuture<'a, Result<T, Error>>,
) -> Result<Option<T>, Error> {
    match cap {
        Some(cap) => optional_capability(f(cap).await),
        None => Ok(None),
    }
}

async fn capability_available_async<C: ?Sized>(
    cap: Option<&C>,
    f: impl for<'a> FnOnce(&'a C) -> BoxFuture<'a, Result<bool, Error>>,
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

/// RX channel handle for asynchronous devices.
pub struct AsyncRxChannel<'a, T: ?Sized> {
    dev: &'a T,
    channel: usize,
}

impl<'a, T: ?Sized> AsyncRxChannel<'a, T> {
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

/// TX channel handle for asynchronous devices.
pub struct AsyncTxChannel<'a, T: ?Sized> {
    dev: &'a T,
    channel: usize,
}

impl<'a, T: ?Sized> AsyncTxChannel<'a, T> {
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

/// Asynchronous antenna control handle for one channel.
pub struct AsyncAntenna<'a, T: AsyncAntennaControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AsyncAntennaControl + ?Sized> AsyncAntenna<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Selectable antenna ports.
    pub fn ports(&self) -> BoxFuture<'_, Result<Vec<String>, Error>> {
        self.dev.async_antennas(self.direction, self.channel)
    }

    /// Currently selected antenna.
    pub fn selected(&self) -> BoxFuture<'_, Result<String, Error>> {
        self.dev.async_antenna(self.direction, self.channel)
    }

    /// Select an antenna port.
    pub fn select<'b>(&'b self, name: &'b str) -> BoxFuture<'b, Result<(), Error>> {
        self.dev
            .async_set_antenna(self.direction, self.channel, name)
    }
}

/// Asynchronous automatic gain control handle for one channel.
pub struct AsyncAgc<'a, T: AsyncAgcControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AsyncAgcControl + ?Sized> AsyncAgc<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    async fn ensure_available(&self) -> Result<(), Error> {
        if self
            .dev
            .async_agc_available(self.direction, self.channel)
            .await?
        {
            Ok(())
        } else {
            Err(Error::unsupported(Capability::Agc))
        }
    }

    /// Return whether automatic gain control is enabled.
    pub fn enabled(&self) -> BoxFuture<'_, Result<bool, Error>> {
        async move {
            self.ensure_available().await?;
            self.dev
                .async_agc_enabled(self.direction, self.channel)
                .await
        }
        .boxed()
    }

    /// Enable automatic gain control.
    pub fn enable(&self) -> BoxFuture<'_, Result<(), Error>> {
        self.set_enabled(true)
    }

    /// Disable automatic gain control.
    pub fn disable(&self) -> BoxFuture<'_, Result<(), Error>> {
        self.set_enabled(false)
    }

    /// Set whether automatic gain control is enabled.
    pub fn set_enabled(&self, enabled: bool) -> BoxFuture<'_, Result<(), Error>> {
        async move {
            self.ensure_available().await?;
            self.dev
                .async_set_agc_enabled(self.direction, self.channel, enabled)
                .await
        }
        .boxed()
    }
}

/// Asynchronous gain control handle for one channel.
pub struct AsyncGain<'a, T: AsyncGainControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AsyncGainControl + ?Sized> AsyncGain<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Named gain elements.
    pub fn elements(&self) -> BoxFuture<'_, Result<Vec<String>, Error>> {
        self.dev.async_gain_elements(self.direction, self.channel)
    }

    /// Overall gain.
    pub fn value(&self) -> BoxFuture<'_, Result<Option<f64>, Error>> {
        self.dev.async_gain(self.direction, self.channel)
    }

    /// Set overall gain.
    pub fn set(&self, gain: f64) -> BoxFuture<'_, Result<(), Error>> {
        self.dev.async_set_gain(self.direction, self.channel, gain)
    }

    /// Overall gain range.
    pub fn range(&self) -> BoxFuture<'_, Result<Range, Error>> {
        self.dev.async_gain_range(self.direction, self.channel)
    }

    /// Named gain element handle.
    pub fn element(&self, name: &str) -> AsyncGainElement<'a, T> {
        AsyncGainElement {
            dev: self.dev,
            direction: self.direction,
            channel: self.channel,
            name: name.to_string(),
        }
    }
}

/// Asynchronous gain element handle for one channel.
pub struct AsyncGainElement<'a, T: AsyncGainControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
    name: String,
}

impl<'a, T: AsyncGainControl + ?Sized> AsyncGainElement<'a, T> {
    /// Gain element value.
    pub fn value(&self) -> BoxFuture<'_, Result<Option<f64>, Error>> {
        self.dev
            .async_gain_element(self.direction, self.channel, &self.name)
    }

    /// Set gain element value.
    pub fn set(&self, gain: f64) -> BoxFuture<'_, Result<(), Error>> {
        self.dev
            .async_set_gain_element(self.direction, self.channel, &self.name, gain)
    }

    /// Gain element range.
    pub fn range(&self) -> BoxFuture<'_, Result<Range, Error>> {
        self.dev
            .async_gain_element_range(self.direction, self.channel, &self.name)
    }
}

/// Asynchronous frequency control handle for one channel.
pub struct AsyncFrequency<'a, T: AsyncFrequencyControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AsyncFrequencyControl + ?Sized> AsyncFrequency<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Overall frequency.
    pub fn value(&self) -> BoxFuture<'_, Result<f64, Error>> {
        self.dev.async_frequency(self.direction, self.channel)
    }

    /// Set overall frequency.
    pub fn set(&self, frequency: f64) -> BoxFuture<'_, Result<(), Error>> {
        self.set_with_args(frequency, Args::new())
    }

    /// Set overall frequency with driver arguments.
    pub fn set_with_args(&self, frequency: f64, args: Args) -> BoxFuture<'_, Result<(), Error>> {
        self.dev
            .async_set_frequency(self.direction, self.channel, frequency, args)
    }

    /// Overall frequency range.
    pub fn range(&self) -> BoxFuture<'_, Result<Range, Error>> {
        self.dev.async_frequency_range(self.direction, self.channel)
    }

    /// Named frequency components.
    pub fn components(&self) -> BoxFuture<'_, Result<Vec<String>, Error>> {
        self.dev
            .async_frequency_components(self.direction, self.channel)
    }

    /// Named frequency component handle.
    pub fn component(&self, name: &str) -> AsyncFrequencyComponent<'a, T> {
        AsyncFrequencyComponent {
            dev: self.dev,
            direction: self.direction,
            channel: self.channel,
            name: name.to_string(),
        }
    }
}

/// Asynchronous frequency component handle for one channel.
pub struct AsyncFrequencyComponent<'a, T: AsyncFrequencyControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
    name: String,
}

impl<'a, T: AsyncFrequencyControl + ?Sized> AsyncFrequencyComponent<'a, T> {
    /// Frequency component value.
    pub fn value(&self) -> BoxFuture<'_, Result<f64, Error>> {
        self.dev
            .async_component_frequency(self.direction, self.channel, &self.name)
    }

    /// Set frequency component value.
    pub fn set(&self, frequency: f64) -> BoxFuture<'_, Result<(), Error>> {
        self.dev
            .async_set_component_frequency(self.direction, self.channel, &self.name, frequency)
    }

    /// Frequency component range.
    pub fn range(&self) -> BoxFuture<'_, Result<Range, Error>> {
        self.dev
            .async_component_frequency_range(self.direction, self.channel, &self.name)
    }
}

/// Asynchronous sample-rate control handle for one channel.
pub struct AsyncSampleRate<'a, T: AsyncSampleRateControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AsyncSampleRateControl + ?Sized> AsyncSampleRate<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Sample rate.
    pub fn value(&self) -> BoxFuture<'_, Result<f64, Error>> {
        self.dev.async_sample_rate(self.direction, self.channel)
    }

    /// Set sample rate.
    pub fn set(&self, rate: f64) -> BoxFuture<'_, Result<(), Error>> {
        self.dev
            .async_set_sample_rate(self.direction, self.channel, rate)
    }

    /// Sample-rate range.
    pub fn range(&self) -> BoxFuture<'_, Result<Range, Error>> {
        self.dev
            .async_get_sample_rate_range(self.direction, self.channel)
    }
}

/// Asynchronous bandwidth control handle for one channel.
pub struct AsyncBandwidth<'a, T: AsyncBandwidthControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AsyncBandwidthControl + ?Sized> AsyncBandwidth<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    /// Bandwidth.
    pub fn value(&self) -> BoxFuture<'_, Result<f64, Error>> {
        self.dev.async_bandwidth(self.direction, self.channel)
    }

    /// Set bandwidth.
    pub fn set(&self, bandwidth: f64) -> BoxFuture<'_, Result<(), Error>> {
        self.dev
            .async_set_bandwidth(self.direction, self.channel, bandwidth)
    }

    /// Bandwidth range.
    pub fn range(&self) -> BoxFuture<'_, Result<Range, Error>> {
        self.dev
            .async_get_bandwidth_range(self.direction, self.channel)
    }
}

/// Asynchronous automatic DC offset correction handle for one channel.
pub struct AsyncDcOffset<'a, T: AsyncDcOffsetControl + ?Sized> {
    dev: &'a T,
    direction: Direction,
    channel: usize,
}

impl<'a, T: AsyncDcOffsetControl + ?Sized> AsyncDcOffset<'a, T> {
    fn new(dev: &'a T, direction: Direction, channel: usize) -> Self {
        Self {
            dev,
            direction,
            channel,
        }
    }

    async fn ensure_available(&self) -> Result<(), Error> {
        if self
            .dev
            .async_dc_offset_available(self.direction, self.channel)
            .await?
        {
            Ok(())
        } else {
            Err(Error::unsupported(Capability::DcOffset))
        }
    }

    /// Return whether automatic DC offset correction is enabled.
    pub fn enabled(&self) -> BoxFuture<'_, Result<bool, Error>> {
        async move {
            self.ensure_available().await?;
            self.dev
                .async_dc_offset_enabled(self.direction, self.channel)
                .await
        }
        .boxed()
    }

    /// Enable automatic DC offset correction.
    pub fn enable(&self) -> BoxFuture<'_, Result<(), Error>> {
        self.set_enabled(true)
    }

    /// Disable automatic DC offset correction.
    pub fn disable(&self) -> BoxFuture<'_, Result<(), Error>> {
        self.set_enabled(false)
    }

    /// Set whether automatic DC offset correction is enabled.
    pub fn set_enabled(&self, enabled: bool) -> BoxFuture<'_, Result<(), Error>> {
        async move {
            self.ensure_available().await?;
            self.dev
                .async_set_dc_offset_enabled(self.direction, self.channel, enabled)
                .await
        }
        .boxed()
    }
}

/// Wraps an asynchronous driver implementation.
#[derive(Clone)]
pub struct AsyncDevice<T> {
    dev: T,
}

impl<T> AsyncDevice<T> {
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

impl<T> AsyncDevice<T>
where
    T: AsyncTypedDeviceBackend,
{
    /// Open a typed asynchronous device matching `args`.
    pub fn from_args<A>(args: A) -> BoxFuture<'static, Result<Self, Error>>
    where
        A: TryInto<Args> + Send + 'static,
    {
        async move {
            let args = args
                .try_into()
                .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
            match args.get::<Driver>("driver") {
                Ok(driver) if driver != <T as AsyncTypedDeviceBackend>::driver() => {
                    return Err(Error::DriverMismatch {
                        expected: <T as AsyncTypedDeviceBackend>::driver(),
                        requested: driver,
                    });
                }
                Ok(_) | Err(Error::MissingArgument { .. }) => {}
                Err(e) => return Err(e),
            }
            Ok(Self::from_impl(T::async_open(&args).await?))
        }
        .boxed()
    }
}

impl<T> AsyncDevice<T>
where
    T: AsyncDynDeviceBackend + 'static,
{
    /// Convert this typed device into a runtime-dispatched asynchronous device.
    pub fn erase(self) -> AsyncDynDevice {
        AsyncDynDevice::from_impl(self.dev)
    }
}

impl<T: AsyncDeviceInfo> AsyncDevice<T> {
    /// SDR driver.
    pub fn driver(&self) -> Driver {
        self.dev.driver()
    }

    /// Identifier for the device, e.g. its serial.
    pub fn id(&self) -> BoxFuture<'_, Result<String, Error>> {
        self.dev.async_id()
    }

    /// Device info that can be displayed to the user.
    pub fn info(&self) -> BoxFuture<'_, Result<Args, Error>> {
        self.dev.async_info()
    }
}

impl AsyncDynDevice {
    /// Open the first discovered runtime-dispatched asynchronous device.
    pub fn new() -> BoxFuture<'static, Result<Self, Error>> {
        async {
            let registry = AsyncRegistry::default();
            let descriptors = registry.probe(Args::new()).await?;
            let descriptor = descriptors.first().ok_or(Error::DeviceNotFound)?;
            registry.open(descriptor).await
        }
        .boxed()
    }

    /// Open a runtime-dispatched asynchronous device matching `args`.
    pub fn from_args<A>(args: A) -> BoxFuture<'static, Result<Self, Error>>
    where
        A: TryInto<Args> + Send + 'static,
    {
        async move {
            let registry = AsyncRegistry::default();
            registry.open_args(args).await
        }
        .boxed()
    }

    /// Create a runtime-dispatched asynchronous device from an implementation.
    pub fn from_impl<T: AsyncDynDeviceBackend + 'static>(dev: T) -> Self {
        Self {
            inner: Arc::new(dev),
        }
    }

    /// Borrow the erased backend.
    pub fn as_backend(&self) -> &dyn AsyncDynDeviceBackend {
        self.inner.as_ref()
    }

    /// Try to downcast to a concrete device implementation.
    pub fn downcast_ref<D: AsyncDeviceInfo + 'static>(&self) -> Option<&D> {
        self.inner.as_any().downcast_ref::<D>()
    }

    /// Try to downcast mutably to a concrete device implementation.
    pub fn downcast_mut<D: AsyncDeviceInfo + 'static>(&mut self) -> Option<&mut D> {
        Arc::get_mut(&mut self.inner)?
            .as_any_mut()
            .downcast_mut::<D>()
    }

    /// SDR driver.
    pub fn driver(&self) -> Driver {
        self.inner.driver()
    }

    /// Identifier for the device, e.g. its serial.
    pub fn id(&self) -> BoxFuture<'_, Result<String, Error>> {
        self.inner.async_id()
    }

    /// Device info that can be displayed to the user.
    pub fn info(&self) -> BoxFuture<'_, Result<Args, Error>> {
        self.inner.async_info()
    }

    /// Structured runtime capabilities for the device.
    pub fn capabilities(&self) -> BoxFuture<'_, Result<DeviceCapabilities, Error>> {
        self.inner.async_capabilities()
    }

    /// RX channel handle.
    pub fn rx(&self, index: usize) -> BoxFuture<'_, Result<AsyncRxChannel<'_, Self>, Error>> {
        async move {
            async_ensure_channel(self, Direction::Rx, index).await?;
            Ok(AsyncRxChannel::new(self, index))
        }
        .boxed()
    }

    /// TX channel handle.
    pub fn tx(&self, index: usize) -> BoxFuture<'_, Result<AsyncTxChannel<'_, Self>, Error>> {
        async move {
            async_ensure_channel(self, Direction::Tx, index).await?;
            Ok(AsyncTxChannel::new(self, index))
        }
        .boxed()
    }

    /// Create an RX streamer.
    pub fn rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
    ) -> BoxFuture<'a, Result<DynAsyncRxStreamer, Error>> {
        self.rx_streamer_with_args(channels, Args::new())
    }

    /// Create an RX streamer, using `args`.
    pub fn rx_streamer_with_args<'a, A>(
        &'a self,
        channels: &'a [usize],
        args: A,
    ) -> BoxFuture<'a, Result<DynAsyncRxStreamer, Error>>
    where
        A: TryInto<Args> + Send + 'a,
    {
        async move {
            for channel in channels {
                async_ensure_channel(self, Direction::Rx, *channel).await?;
            }
            <Self as AsyncRxDevice>::async_rx_streamer(
                self,
                channels,
                args.try_into()
                    .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?,
            )
            .await
        }
        .boxed()
    }

    /// Create a TX streamer.
    pub fn tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
    ) -> BoxFuture<'a, Result<DynAsyncTxStreamer, Error>> {
        self.tx_streamer_with_args(channels, Args::new())
    }

    /// Create a TX streamer, using `args`.
    pub fn tx_streamer_with_args<'a, A>(
        &'a self,
        channels: &'a [usize],
        args: A,
    ) -> BoxFuture<'a, Result<DynAsyncTxStreamer, Error>>
    where
        A: TryInto<Args> + Send + 'a,
    {
        async move {
            for channel in channels {
                async_ensure_channel(self, Direction::Tx, *channel).await?;
            }
            <Self as AsyncTxDevice>::async_tx_streamer(
                self,
                channels,
                args.try_into()
                    .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?,
            )
            .await
        }
        .boxed()
    }
}

impl AsyncDeviceInfo for AsyncDynDevice {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn driver(&self) -> Driver {
        self.inner.driver()
    }

    fn async_id(&self) -> BoxFuture<'_, Result<String, Error>> {
        self.inner.async_id()
    }

    fn async_info(&self) -> BoxFuture<'_, Result<Args, Error>> {
        self.inner.async_info()
    }
}

impl AsyncChannelInfo for AsyncDynDevice {
    fn async_num_channels(&self, direction: Direction) -> BoxFuture<'_, Result<usize, Error>> {
        async move {
            self.inner
                .async_channel_info()
                .ok_or_else(|| Error::unsupported(Capability::ChannelInfo))?
                .async_num_channels(direction)
                .await
        }
        .boxed()
    }

    fn async_full_duplex(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .async_channel_info()
                .ok_or_else(|| Error::unsupported(Capability::ChannelInfo))?
                .async_full_duplex(direction, channel)
                .await
        }
        .boxed()
    }
}

impl AsyncRxDevice for AsyncDynDevice {
    type RxStreamer = DynAsyncRxStreamer;

    fn async_rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<Self::RxStreamer, Error>> {
        async move {
            self.inner
                .async_rx_device()
                .ok_or_else(|| Error::unsupported(Capability::RxStreaming))?
                .async_rx_streamer(channels, args)
                .await
        }
        .boxed()
    }
}

impl AsyncTxDevice for AsyncDynDevice {
    type TxStreamer = DynAsyncTxStreamer;

    fn async_tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<Self::TxStreamer, Error>> {
        async move {
            self.inner
                .async_tx_device()
                .ok_or_else(|| Error::unsupported(Capability::TxStreaming))?
                .async_tx_streamer(channels, args)
                .await
        }
        .boxed()
    }
}

macro_rules! impl_dyn_control {
    ($trait_name:ident, $accessor:ident, $cap:expr, $($body:item),* $(,)?) => {
        impl $trait_name for AsyncDynDevice {
            $($body)*
        }
    };
}

impl_dyn_control!(
    AsyncAntennaControl,
    async_antenna_control,
    Capability::Antenna,
    fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Vec<String>, Error>> {
        async move {
            self.inner
                .async_antenna_control()
                .ok_or_else(|| Error::unsupported(Capability::Antenna))?
                .async_antennas(direction, channel)
                .await
        }
        .boxed()
    },
    fn async_antenna(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<String, Error>> {
        async move {
            self.inner
                .async_antenna_control()
                .ok_or_else(|| Error::unsupported(Capability::Antenna))?
                .async_antenna(direction, channel)
                .await
        }
        .boxed()
    },
    fn async_set_antenna<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<(), Error>> {
        async move {
            self.inner
                .async_antenna_control()
                .ok_or_else(|| Error::unsupported(Capability::Antenna))?
                .async_set_antenna(direction, channel, name)
                .await
        }
        .boxed()
    }
);

impl AsyncAgcControl for AsyncDynDevice {
    fn async_agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .async_agc_control()
                .ok_or_else(|| Error::unsupported(Capability::Agc))?
                .async_agc_available(direction, channel)
                .await
        }
        .boxed()
    }

    fn async_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .async_agc_control()
                .ok_or_else(|| Error::unsupported(Capability::Agc))?
                .async_agc_enabled(direction, channel)
                .await
        }
        .boxed()
    }

    fn async_set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> BoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .async_agc_control()
                .ok_or_else(|| Error::unsupported(Capability::Agc))?
                .async_set_agc_enabled(direction, channel, enabled)
                .await
        }
        .boxed()
    }
}

impl AsyncGainControl for AsyncDynDevice {
    fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Vec<String>, Error>> {
        async move {
            self.inner
                .async_gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .async_gain_elements(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> BoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .async_gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .async_set_gain(direction, channel, gain)
                .await
        }
        .boxed()
    }
    fn async_gain(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Option<f64>, Error>> {
        async move {
            self.inner
                .async_gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .async_gain(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_gain_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Range, Error>> {
        async move {
            self.inner
                .async_gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .async_gain_range(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_set_gain_element<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
        gain: f64,
    ) -> BoxFuture<'a, Result<(), Error>> {
        async move {
            self.inner
                .async_gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .async_set_gain_element(direction, channel, name, gain)
                .await
        }
        .boxed()
    }
    fn async_gain_element<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Option<f64>, Error>> {
        async move {
            self.inner
                .async_gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .async_gain_element(direction, channel, name)
                .await
        }
        .boxed()
    }
    fn async_gain_element_range<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Range, Error>> {
        async move {
            self.inner
                .async_gain_control()
                .ok_or_else(|| Error::unsupported(Capability::Gain))?
                .async_gain_element_range(direction, channel, name)
                .await
        }
        .boxed()
    }
}

impl AsyncFrequencyControl for AsyncDynDevice {
    fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Range, Error>> {
        async move {
            self.inner
                .async_frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .async_frequency_range(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_frequency(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<f64, Error>> {
        async move {
            self.inner
                .async_frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .async_frequency(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> BoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .async_frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .async_set_frequency(direction, channel, frequency, args)
                .await
        }
        .boxed()
    }
    fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Vec<String>, Error>> {
        async move {
            self.inner
                .async_frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .async_frequency_components(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_component_frequency_range<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Range, Error>> {
        async move {
            self.inner
                .async_frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .async_component_frequency_range(direction, channel, name)
                .await
        }
        .boxed()
    }
    fn async_component_frequency<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> BoxFuture<'a, Result<f64, Error>> {
        async move {
            self.inner
                .async_frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .async_component_frequency(direction, channel, name)
                .await
        }
        .boxed()
    }
    fn async_set_component_frequency<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
        frequency: f64,
    ) -> BoxFuture<'a, Result<(), Error>> {
        async move {
            self.inner
                .async_frequency_control()
                .ok_or_else(|| Error::unsupported(Capability::Frequency))?
                .async_set_component_frequency(direction, channel, name, frequency)
                .await
        }
        .boxed()
    }
}

impl AsyncSampleRateControl for AsyncDynDevice {
    fn async_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<f64, Error>> {
        async move {
            self.inner
                .async_sample_rate_control()
                .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
                .async_sample_rate(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> BoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .async_sample_rate_control()
                .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
                .async_set_sample_rate(direction, channel, rate)
                .await
        }
        .boxed()
    }
    fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Range, Error>> {
        async move {
            self.inner
                .async_sample_rate_control()
                .ok_or_else(|| Error::unsupported(Capability::SampleRate))?
                .async_get_sample_rate_range(direction, channel)
                .await
        }
        .boxed()
    }
}

impl AsyncBandwidthControl for AsyncDynDevice {
    fn async_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<f64, Error>> {
        async move {
            self.inner
                .async_bandwidth_control()
                .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
                .async_bandwidth(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bandwidth: f64,
    ) -> BoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .async_bandwidth_control()
                .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
                .async_set_bandwidth(direction, channel, bandwidth)
                .await
        }
        .boxed()
    }
    fn async_get_bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<Range, Error>> {
        async move {
            self.inner
                .async_bandwidth_control()
                .ok_or_else(|| Error::unsupported(Capability::Bandwidth))?
                .async_get_bandwidth_range(direction, channel)
                .await
        }
        .boxed()
    }
}

impl AsyncDcOffsetControl for AsyncDynDevice {
    fn async_dc_offset_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .async_dc_offset_control()
                .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
                .async_dc_offset_available(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> BoxFuture<'_, Result<bool, Error>> {
        async move {
            self.inner
                .async_dc_offset_control()
                .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
                .async_dc_offset_enabled(direction, channel)
                .await
        }
        .boxed()
    }
    fn async_set_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> BoxFuture<'_, Result<(), Error>> {
        async move {
            self.inner
                .async_dc_offset_control()
                .ok_or_else(|| Error::unsupported(Capability::DcOffset))?
                .async_set_dc_offset_enabled(direction, channel, enabled)
                .await
        }
        .boxed()
    }
}

impl<T: AsyncChannelInfo> AsyncDevice<T> {
    /// RX channel handle.
    pub fn rx(&self, index: usize) -> BoxFuture<'_, Result<AsyncRxChannel<'_, T>, Error>> {
        async move {
            async_ensure_channel(&self.dev, Direction::Rx, index).await?;
            Ok(AsyncRxChannel::new(&self.dev, index))
        }
        .boxed()
    }

    /// TX channel handle.
    pub fn tx(&self, index: usize) -> BoxFuture<'_, Result<AsyncTxChannel<'_, T>, Error>> {
        async move {
            async_ensure_channel(&self.dev, Direction::Tx, index).await?;
            Ok(AsyncTxChannel::new(&self.dev, index))
        }
        .boxed()
    }
}

async fn async_ensure_channel<T>(dev: &T, direction: Direction, channel: usize) -> Result<(), Error>
where
    T: AsyncChannelInfo + ?Sized,
{
    let available = dev.async_num_channels(direction).await?;
    if channel < available {
        Ok(())
    } else {
        Err(Error::invalid_channel(direction, channel, available))
    }
}

impl<T: AsyncRxDevice + AsyncChannelInfo> AsyncDevice<T> {
    /// Create an RX streamer over one or more RX channels.
    pub fn rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
    ) -> BoxFuture<'a, Result<T::RxStreamer, Error>> {
        self.rx_streamer_with_args(channels, Args::new())
    }

    /// Create an RX streamer over one or more RX channels, using `args`.
    pub fn rx_streamer_with_args<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<T::RxStreamer, Error>> {
        async move {
            for channel in channels {
                async_ensure_channel(&self.dev, Direction::Rx, *channel).await?;
            }
            self.dev.async_rx_streamer(channels, args).await
        }
        .boxed()
    }
}

impl<T: AsyncTxDevice + AsyncChannelInfo> AsyncDevice<T> {
    /// Create a TX streamer over one or more TX channels.
    pub fn tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
    ) -> BoxFuture<'a, Result<T::TxStreamer, Error>> {
        self.tx_streamer_with_args(channels, Args::new())
    }

    /// Create a TX streamer over one or more TX channels, using `args`.
    pub fn tx_streamer_with_args<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> BoxFuture<'a, Result<T::TxStreamer, Error>> {
        async move {
            for channel in channels {
                async_ensure_channel(&self.dev, Direction::Tx, *channel).await?;
            }
            self.dev.async_tx_streamer(channels, args).await
        }
        .boxed()
    }
}

impl<'a, T: AsyncRxDevice + ?Sized> AsyncRxChannel<'a, T> {
    /// Create a single-channel RX streamer.
    pub fn streamer(&self) -> BoxFuture<'_, Result<T::RxStreamer, Error>> {
        self.streamer_with_args(Args::new())
    }

    /// Create a single-channel RX streamer, using `args`.
    pub fn streamer_with_args(&self, args: Args) -> BoxFuture<'_, Result<T::RxStreamer, Error>> {
        async move {
            let channels = [self.channel];
            self.dev.async_rx_streamer(&channels, args).await
        }
        .boxed()
    }
}

impl<'a, T: AsyncTxDevice + ?Sized> AsyncTxChannel<'a, T> {
    /// Create a single-channel TX streamer.
    pub fn streamer(&self) -> BoxFuture<'_, Result<T::TxStreamer, Error>> {
        self.streamer_with_args(Args::new())
    }

    /// Create a single-channel TX streamer, using `args`.
    pub fn streamer_with_args(&self, args: Args) -> BoxFuture<'_, Result<T::TxStreamer, Error>> {
        async move {
            let channels = [self.channel];
            self.dev.async_tx_streamer(&channels, args).await
        }
        .boxed()
    }
}

impl<'a, T: AsyncChannelInfo + ?Sized> AsyncRxChannel<'a, T> {
    /// Full-duplex support for this RX channel.
    pub fn full_duplex(&self) -> BoxFuture<'_, Result<bool, Error>> {
        self.dev.async_full_duplex(Direction::Rx, self.channel)
    }
}

impl<'a, T: AsyncChannelInfo + ?Sized> AsyncTxChannel<'a, T> {
    /// Full-duplex support for this TX channel.
    pub fn full_duplex(&self) -> BoxFuture<'_, Result<bool, Error>> {
        self.dev.async_full_duplex(Direction::Tx, self.channel)
    }
}

macro_rules! impl_async_channel_controls {
    ($channel:ident, $direction:expr) => {
        impl<'a, T: AsyncAntennaControl + ?Sized> $channel<'a, T> {
            /// Antenna control.
            pub fn antenna(&self) -> AsyncAntenna<'_, T> {
                AsyncAntenna::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: AsyncAgcControl + ?Sized> $channel<'a, T> {
            /// Automatic gain control.
            pub fn agc(&self) -> AsyncAgc<'_, T> {
                AsyncAgc::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: AsyncGainControl + ?Sized> $channel<'a, T> {
            /// Gain control.
            pub fn gain(&self) -> AsyncGain<'_, T> {
                AsyncGain::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: AsyncFrequencyControl + ?Sized> $channel<'a, T> {
            /// Frequency control.
            pub fn frequency(&self) -> AsyncFrequency<'_, T> {
                AsyncFrequency::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: AsyncSampleRateControl + ?Sized> $channel<'a, T> {
            /// Sample-rate control.
            pub fn sample_rate(&self) -> AsyncSampleRate<'_, T> {
                AsyncSampleRate::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: AsyncBandwidthControl + ?Sized> $channel<'a, T> {
            /// Bandwidth control.
            pub fn bandwidth(&self) -> AsyncBandwidth<'_, T> {
                AsyncBandwidth::new(self.dev, $direction, self.channel)
            }
        }

        impl<'a, T: AsyncDcOffsetControl + ?Sized> $channel<'a, T> {
            /// Automatic DC offset correction.
            pub fn dc_offset(&self) -> AsyncDcOffset<'_, T> {
                AsyncDcOffset::new(self.dev, $direction, self.channel)
            }
        }
    };
}

impl_async_channel_controls!(AsyncRxChannel, Direction::Rx);
impl_async_channel_controls!(AsyncTxChannel, Direction::Tx);

/// Asynchronous driver discovery/opening backend.
pub trait AsyncDriverBackend: Send + Sync {
    /// Driver handled by this backend.
    fn driver(&self) -> Driver;
    /// Probe devices matching `args`.
    fn probe<'a>(&'a self, args: &'a Args) -> BoxFuture<'a, Result<Vec<DeviceDescriptor>, Error>>;
    /// Open a previously discovered device descriptor.
    fn open<'a>(
        &'a self,
        descriptor: &'a DeviceDescriptor,
    ) -> BoxFuture<'a, Result<AsyncDynDevice, Error>>;
}

/// Typed asynchronous driver implementation that can be opened directly.
pub trait AsyncTypedDeviceBackend: AsyncDynDeviceBackend + Sized + 'static {
    /// Driver implemented by this backend.
    fn driver() -> Driver;
    /// Probe devices matching `args`.
    fn async_probe(args: &Args) -> BoxFuture<'_, Result<Vec<Args>, Error>>;
    /// Open a typed device matching `args`.
    fn async_open(args: &Args) -> BoxFuture<'_, Result<Self, Error>>;
}

/// Registry of asynchronous driver discovery/opening backends.
pub struct AsyncRegistry {
    backends: Vec<Box<dyn AsyncDriverBackend>>,
}

impl AsyncRegistry {
    /// Create an empty asynchronous registry.
    pub fn empty() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Register an asynchronous driver backend.
    pub fn register<B>(&mut self, backend: B) -> &mut Self
    where
        B: AsyncDriverBackend + 'static,
    {
        self.backends.push(Box::new(backend));
        self
    }

    /// Probe devices matching `args`.
    pub fn probe<'a, A>(&'a self, args: A) -> BoxFuture<'a, Result<Vec<DeviceDescriptor>, Error>>
    where
        A: TryInto<Args> + Send + 'a,
    {
        async move {
            let args = args
                .try_into()
                .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
            let driver = requested_driver(&args)?;
            let mut descriptors = Vec::new();
            let mut matched_backend = false;

            for backend in &self.backends {
                if driver.is_none() || driver == Some(backend.driver()) {
                    matched_backend = true;
                    descriptors.append(&mut backend.probe(&args).await?);
                }
            }

            if let Some(driver) = driver {
                if !matched_backend {
                    if async_builtin_driver_enabled(driver) {
                        return Err(Error::unsupported_reason(
                            Capability::DriverOperation,
                            format!("driver {driver:?} does not expose an async API"),
                        ));
                    }
                    return Err(Error::DriverFeatureNotEnabled { driver });
                }
            }

            Ok(descriptors)
        }
        .boxed()
    }

    /// Open a discovered device descriptor.
    pub fn open<'a>(
        &'a self,
        descriptor: &'a DeviceDescriptor,
    ) -> BoxFuture<'a, Result<AsyncDynDevice, Error>> {
        async move {
            let driver = descriptor.driver();
            let mut matched_backend = false;

            for backend in &self.backends {
                if backend.driver() != driver {
                    continue;
                }
                matched_backend = true;
                match backend.open(descriptor).await {
                    Ok(device) => return Ok(device),
                    Err(Error::DeviceNotFound) => {}
                    Err(e) => return Err(e),
                }
            }

            if !matched_backend {
                if async_builtin_driver_enabled(driver) {
                    return Err(Error::unsupported_reason(
                        Capability::DriverOperation,
                        format!("driver {driver:?} does not expose an async API"),
                    ));
                }
                return Err(Error::DriverFeatureNotEnabled { driver });
            }

            Err(Error::DeviceNotFound)
        }
        .boxed()
    }

    /// Open the first asynchronous device matching `args`.
    pub fn open_args<'a, A>(&'a self, args: A) -> BoxFuture<'a, Result<AsyncDynDevice, Error>>
    where
        A: TryInto<Args> + Send + 'a,
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
        .boxed()
    }
}

impl Default for AsyncRegistry {
    fn default() -> Self {
        #[allow(unused_mut)]
        let mut registry = Self::empty();

        #[cfg(feature = "dummy")]
        registry.register(BuiltinAsyncDriver::<crate::impls::Dummy>::new(
            Driver::Dummy,
        ));

        registry
    }
}

fn requested_driver(args: &Args) -> Result<Option<Driver>, Error> {
    match args.get::<Driver>("driver") {
        Ok(driver) => Ok(Some(driver)),
        Err(Error::MissingArgument { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

fn async_builtin_driver_enabled(driver: Driver) -> bool {
    match driver {
        Driver::AaroniaHttp => cfg!(all(feature = "aaronia_http", not(target_arch = "wasm32"))),
        Driver::BladeRf => cfg!(all(feature = "bladerf1", not(target_arch = "wasm32"))),
        Driver::Dummy => cfg!(feature = "dummy"),
        Driver::HackRf => cfg!(all(feature = "hackrfone", not(target_arch = "wasm32"))),
        Driver::HydraSdr => cfg!(all(feature = "hydrasdr", not(target_arch = "wasm32"))),
        Driver::RtlSdr => cfg!(all(feature = "rtlsdr", not(target_arch = "wasm32"))),
        Driver::Soapy => cfg!(all(feature = "soapy", not(target_arch = "wasm32"))),
    }
}

#[cfg(feature = "dummy")]
struct BuiltinAsyncDriver<D> {
    driver: Driver,
    _device: std::marker::PhantomData<D>,
}

#[cfg(feature = "dummy")]
impl<D> BuiltinAsyncDriver<D> {
    fn new(driver: Driver) -> Self {
        Self {
            driver,
            _device: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "dummy")]
impl<D> AsyncDriverBackend for BuiltinAsyncDriver<D>
where
    D: AsyncTypedDeviceBackend + Send + Sync,
{
    fn driver(&self) -> Driver {
        self.driver
    }

    fn probe<'a>(&'a self, args: &'a Args) -> BoxFuture<'a, Result<Vec<DeviceDescriptor>, Error>> {
        async move {
            D::async_probe(args).await.map(|descriptors| {
                descriptors
                    .into_iter()
                    .map(|args| DeviceDescriptor::new(self.driver, args))
                    .collect()
            })
        }
        .boxed()
    }

    fn open<'a>(
        &'a self,
        descriptor: &'a DeviceDescriptor,
    ) -> BoxFuture<'a, Result<AsyncDynDevice, Error>> {
        async move {
            Ok(AsyncDynDevice::from_impl(
                D::async_open(descriptor.args()).await?,
            ))
        }
        .boxed()
    }
}

#[cfg(all(test, feature = "dummy"))]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use num_complex::Complex32;

    #[test]
    fn async_dyn_dummy_controls_and_streams() {
        block_on(async {
            let dev = AsyncDynDevice::from_args("driver=dummy").await.unwrap();
            let rx0 = dev.rx(0).await.unwrap();

            rx0.sample_rate().set(1.0e6).await.unwrap();
            rx0.frequency().set(100.0e6).await.unwrap();
            rx0.agc().disable().await.unwrap();
            rx0.gain().set(12.0).await.unwrap();

            assert_eq!(rx0.sample_rate().value().await.unwrap(), 1.0e6);
            assert_eq!(rx0.frequency().value().await.unwrap(), 100.0e6);
            assert_eq!(rx0.gain().value().await.unwrap(), Some(12.0));

            let mut samples = [Complex32::new(1.0, 1.0); 16];
            let mut rx = rx0.streamer().await.unwrap();
            rx.activate().await.unwrap();
            let n = rx.read(&mut [&mut samples], 200_000).await.unwrap();
            assert_eq!(n, samples.len());
            assert!(samples
                .iter()
                .all(|sample| *sample == Complex32::new(0.0, 0.0)));

            let tx0 = dev.tx(0).await.unwrap();
            let mut tx = tx0.streamer().await.unwrap();
            tx.activate().await.unwrap();
            let n = tx.write(&[&samples], None, true, 200_000).await.unwrap();
            assert_eq!(n, samples.len());
        });
    }

    #[test]
    fn async_typed_dummy_erases_and_reports_capabilities() {
        block_on(async {
            let dev = AsyncDevice::<crate::impls::Dummy>::from_args("driver=dummy")
                .await
                .unwrap();
            let dev = dev.erase();

            assert_eq!(dev.driver(), Driver::Dummy);
            assert!(dev.downcast_ref::<crate::impls::Dummy>().is_some());

            let capabilities = dev.capabilities().await.unwrap();
            assert_eq!(capabilities.rx_channels.len(), 1);
            assert_eq!(capabilities.tx_channels.len(), 1);
            assert_eq!(
                capabilities.rx_channels[0].controls.antennas,
                Some(vec!["A".to_string()])
            );
            assert!(capabilities.rx_channels[0].controls.agc);
        });
    }
}
