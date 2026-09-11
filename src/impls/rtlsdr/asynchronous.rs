use num_complex::Complex32;
use rtlsdr_nusb::{Device as RtlSdrDevice, RxStream};
use std::future::IntoFuture;

use super::common::*;
#[cfg(target_arch = "wasm32")]
use crate::dev::WebUsbDeviceFilter;
use crate::Direction::*;
use crate::{
    async_compat::{timeout_from_micros, with_timeout, Shared, TimeoutResult},
    dev::AsyncTypedDeviceBackend,
    Args, AsyncAgcControl, AsyncAntennaControl, AsyncDeviceInfo, AsyncFrequencyControl,
    AsyncGainControl, AsyncRxDevice, AsyncSampleRateControl, Capability, Direction, Driver, Error,
    Range,
};

/// Asynchronous RTL-SDR device backend.
#[derive(Clone)]
pub struct AsyncRtlSdr {
    device_slot: Shared<AsyncSlot<Box<RtlSdrDevice>>>,
    abandoned_stream_slot: Shared<AsyncSlot<RxStream>>,
    metadata: Args,
    inner: Shared<ReceiverContext>,
}

/// RTL-SDR asynchronous receive streamer.
///
/// The streamer shares ownership of the hardware with the device, which remains
/// available for control operations while reception is active. Dropping a
/// streamer defers final close to the next asynchronous device operation.
#[must_use = "deactivate the RTL-SDR stream before dropping it"]
pub struct AsyncRtlSdrRxStreamer {
    abandoned_stream_slot: Shared<AsyncSlot<RxStream>>,
    stream: Option<RxStream>,
    active: bool,
    cleanup_required: bool,
}

#[cfg(not(target_arch = "wasm32"))]
struct AsyncSlot<T>(std::sync::Mutex<Option<T>>);

#[cfg(target_arch = "wasm32")]
struct AsyncSlot<T>(std::cell::RefCell<Option<T>>);

impl<T> AsyncSlot<T> {
    fn new(value: T) -> Self {
        Self(Default::default()).with_value(value)
    }

    fn empty() -> Self {
        Self(Default::default())
    }

    fn with_value(self, value: T) -> Self {
        let result = self.put(value);
        debug_assert!(result.is_ok());
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn take(&self) -> Option<T> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    #[cfg(target_arch = "wasm32")]
    fn take(&self) -> Option<T> {
        self.0.borrow_mut().take()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn put(&self, value: T) -> Result<(), T> {
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_some() {
            Err(value)
        } else {
            *slot = Some(value);
            Ok(())
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn put(&self, value: T) -> Result<(), T> {
        let mut slot = self.0.borrow_mut();
        if slot.is_some() {
            Err(value)
        } else {
            *slot = Some(value);
            Ok(())
        }
    }
}

struct AsyncSlotLease<T> {
    slot: Shared<AsyncSlot<T>>,
    value: Option<T>,
}

impl<T> AsyncSlotLease<T> {
    fn acquire(slot: &Shared<AsyncSlot<T>>) -> Result<Self, Error> {
        let value = slot.take().ok_or(Error::Busy)?;
        Ok(Self {
            slot: Shared::clone(slot),
            value: Some(value),
        })
    }

    fn try_acquire(slot: &Shared<AsyncSlot<T>>) -> Option<Self> {
        Some(Self {
            slot: Shared::clone(slot),
            value: Some(slot.take()?),
        })
    }

    fn value_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("slot lease always owns a value")
    }

    fn value(&self) -> &T {
        self.value.as_ref().expect("slot lease always owns a value")
    }

    fn into_value(mut self) -> T {
        self.value.take().expect("slot lease always owns a value")
    }
}

impl<T> Drop for AsyncSlotLease<T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            let result = self.slot.put(value);
            debug_assert!(result.is_ok(), "async slot was unexpectedly occupied");
        }
    }
}

async fn cleanup_abandoned_stream(slot: &Shared<AsyncSlot<RxStream>>) -> Result<(), Error> {
    let Some(stream) = AsyncSlotLease::try_acquire(slot) else {
        return Ok(());
    };
    // Close consumes the stream and releases even a dormant or stopped claim.
    // Its owned operation handles best-effort cleanup on failure/cancellation.
    stream
        .into_value()
        .close()
        .await
        .map_err(map_rtlsdr_error)?;
    Ok(())
}

impl AsyncRtlSdr {
    /// Return descriptors for detected RTL-SDR devices asynchronously.
    ///
    /// USB serials are preserved as strings. An `index` selector takes
    /// precedence over `serial`, as it does when opening a device.
    pub async fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let devices = RtlSdrDevice::list().await.map_err(map_rtlsdr_error)?;
        probe_args(args, devices)
    }

    /// Open an RTL-SDR device from arguments asynchronously.
    pub async fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let dev = selected_builder(&args)?
            .open()
            .await
            .map_err(map_rtlsdr_error)?;
        let receiver_context = ReceiverContext::from_device_info(dev.info());
        let metadata = device_args(dev.info());

        Ok(Self {
            device_slot: Shared::new(AsyncSlot::new(Box::new(dev))),
            abandoned_stream_slot: Shared::new(AsyncSlot::empty()),
            metadata,
            inner: Shared::new(receiver_context),
        })
    }

    async fn lease_device(&self) -> Result<AsyncSlotLease<Box<RtlSdrDevice>>, Error> {
        let device = AsyncSlotLease::acquire(&self.device_slot)?;
        cleanup_abandoned_stream(&self.abandoned_stream_slot).await?;
        Ok(device)
    }
}

impl AsyncRtlSdr {
    fn driver(&self) -> Driver {
        Driver::RtlSdr
    }

    async fn id(&self) -> Result<String, Error> {
        self.metadata.get::<String>("index")
    }

    async fn info(&self) -> Result<Args, Error> {
        Ok(self.metadata.clone())
    }

    async fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        match direction {
            Rx => Ok(1),
            Tx => Ok(0),
        }
    }

    async fn full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }

    async fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(self.inner.antennas())
    }

    async fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        check_rx(direction, channel)?;
        Ok("RX".to_owned())
    }

    async fn set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        if name != "RX" {
            return Err(Error::invalid_argument(
                "antenna",
                "RTL-SDR exposes only RX",
            ));
        }
        Ok(())
    }

    async fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(true)
    }

    async fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let mut device = self.lease_device().await?;
        let gain = agc_gain_config(device.value().config(), agc)?;
        device
            .value_mut()
            .set_gain(gain)
            .await
            .map_err(map_rtlsdr_error)
    }

    async fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        let device = self.lease_device().await?;
        self.inner.agc_enabled(device.value().config())
    }

    async fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(self
            .inner
            .gains
            .iter()
            .map(|gain| gain.name.to_string())
            .collect())
    }

    async fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let range = overall_gain_range();
        if !range.contains(gain) {
            return Err(Error::out_of_range("gain", range, gain));
        }

        let mut device = self.lease_device().await?;
        device
            .value_mut()
            .set_gain(overall_gain_config(gain))
            .await
            .map_err(map_rtlsdr_error)
    }

    async fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        check_rx(direction, channel)?;
        let device = self.lease_device().await?;
        self.inner.overall_gain(device.value().config())
    }

    async fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(overall_gain_range())
    }

    async fn set_gain_element(
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
        let range = self.gain_element_range(direction, channel, name).await?;
        if !range.contains(gain) {
            return Err(Error::out_of_range("gain", range, gain));
        }

        let mut device = self.lease_device().await?;
        let gain = gain_type.update(device.value().config(), gain)?;
        device
            .value_mut()
            .set_gain(gain)
            .await
            .map_err(map_rtlsdr_error)
    }

    async fn gain_element(
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
        let device = self.lease_device().await?;
        self.inner.gain_value(device.value().config(), gain_type)
    }

    async fn gain_element_range(
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

    async fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.component_frequency_range(direction, channel, "TUNER")
            .await
    }

    async fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.component_frequency(direction, channel, "TUNER").await
    }

    async fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        self.set_component_frequency(direction, channel, "TUNER", frequency)
            .await
    }

    async fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(vec!["TUNER".to_string()])
    }

    async fn component_frequency_range(
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

    async fn component_frequency(
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
        let device = self.lease_device().await?;
        Ok(device.value().actual_frequency_hz() as f64)
    }

    async fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        let range = self
            .component_frequency_range(direction, channel, name)
            .await?;
        if !range.contains(frequency) {
            return Err(Error::out_of_range("frequency", range, frequency));
        }
        let mut device = self.lease_device().await?;
        device
            .value_mut()
            .set_frequency_hz(frequency as u64)
            .await
            .map_err(map_rtlsdr_error)
    }

    async fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        let device = self.lease_device().await?;
        Ok(device.value().actual_sample_rate_hz() as f64)
    }

    async fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let range = self.get_sample_rate_range(direction, channel).await?;
        if !range.contains(rate) {
            return Err(Error::out_of_range("sample_rate", range, rate));
        }
        let mut device = self.lease_device().await?;
        device
            .value_mut()
            .set_sample_rate_hz(rate as u32)
            .await
            .map_err(map_rtlsdr_error)
    }

    async fn get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        Ok(sample_rate_range())
    }
}

impl AsyncDeviceInfo for AsyncRtlSdr {
    fn driver(&self) -> Driver {
        AsyncRtlSdr::driver(self)
    }

    async fn async_id(&self) -> Result<String, Error> {
        AsyncRtlSdr::id(self).await
    }

    async fn async_info(&self) -> Result<Args, Error> {
        AsyncRtlSdr::info(self).await
    }

    async fn async_num_channels(&self, direction: Direction) -> Result<usize, Error> {
        AsyncRtlSdr::num_channels(self, direction).await
    }

    async fn async_full_duplex(&self) -> Result<bool, Error> {
        AsyncRtlSdr::full_duplex(self).await
    }
}

crate::impl_dyn_async_device_backend!(
    AsyncRtlSdr => [rx, antenna, agc, gain, frequency, sample_rate]
);

impl AsyncRxDevice for AsyncRtlSdr {
    type RxStreamer = AsyncRtlSdrRxStreamer;

    async fn async_rx_streamer(
        &self,
        channels: &[usize],
        _args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "rtlsdr",
                "invalid RTL-SDR argument",
            ));
        }
        let device = self.lease_device().await?;
        let stream = device.value().rx_stream().map_err(map_rtlsdr_error)?;
        Ok(AsyncRtlSdrRxStreamer::new(
            Shared::clone(&self.abandoned_stream_slot),
            stream,
        ))
    }
}

impl AsyncAntennaControl for AsyncRtlSdr {
    async fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncRtlSdr::antennas(self, direction, channel).await
    }

    async fn async_antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        AsyncRtlSdr::antenna(self, direction, channel).await
    }

    async fn async_set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        AsyncRtlSdr::set_antenna(self, direction, channel, name).await
    }
}

impl AsyncAgcControl for AsyncRtlSdr {
    async fn async_agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<bool, Error> {
        AsyncRtlSdr::agc_available(self, direction, channel).await
    }

    async fn async_agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        AsyncRtlSdr::agc_enabled(self, direction, channel).await
    }

    async fn async_set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        AsyncRtlSdr::set_agc_enabled(self, direction, channel, enabled).await
    }
}

impl AsyncGainControl for AsyncRtlSdr {
    async fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncRtlSdr::gain_elements(self, direction, channel).await
    }

    async fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        AsyncRtlSdr::set_gain(self, direction, channel, gain).await
    }

    async fn async_gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        AsyncRtlSdr::gain(self, direction, channel).await
    }

    async fn async_gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        AsyncRtlSdr::gain_range(self, direction, channel).await
    }

    async fn async_set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        AsyncRtlSdr::set_gain_element(self, direction, channel, name, gain).await
    }

    async fn async_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        AsyncRtlSdr::gain_element(self, direction, channel, name).await
    }

    async fn async_gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AsyncRtlSdr::gain_element_range(self, direction, channel, name).await
    }
}

impl AsyncFrequencyControl for AsyncRtlSdr {
    async fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncRtlSdr::frequency_range(self, direction, channel).await
    }

    async fn async_frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncRtlSdr::frequency(self, direction, channel).await
    }

    async fn async_set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        AsyncRtlSdr::set_frequency(self, direction, channel, frequency, args).await
    }

    async fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncRtlSdr::frequency_components(self, direction, channel).await
    }

    async fn async_component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AsyncRtlSdr::component_frequency_range(self, direction, channel, name).await
    }

    async fn async_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        AsyncRtlSdr::component_frequency(self, direction, channel, name).await
    }

    async fn async_set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        AsyncRtlSdr::set_component_frequency(self, direction, channel, name, frequency).await
    }
}

impl AsyncSampleRateControl for AsyncRtlSdr {
    async fn async_sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncRtlSdr::sample_rate(self, direction, channel).await
    }

    async fn async_set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        AsyncRtlSdr::set_sample_rate(self, direction, channel, rate).await
    }

    async fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncRtlSdr::get_sample_rate_range(self, direction, channel).await
    }
}

impl AsyncRtlSdrRxStreamer {
    fn new(abandoned_stream_slot: Shared<AsyncSlot<RxStream>>, stream: RxStream) -> Self {
        Self {
            abandoned_stream_slot,
            stream: Some(stream),
            active: false,
            cleanup_required: false,
        }
    }
}

impl crate::AsyncRxStreamer for AsyncRtlSdrRxStreamer {
    async fn mtu(&self) -> Result<usize, Error> {
        Ok(F32_RX_MTU)
    }

    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        if self.active {
            return Ok(());
        }
        self.cleanup_required = true;
        self.stream
            .as_mut()
            .ok_or(Error::DeviceDisconnected)?
            .start()
            .await
            .map_err(map_rtlsdr_error)?;
        self.active = true;
        Ok(())
    }

    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        if self.cleanup_required {
            self.active = false;
            self.stream
                .as_mut()
                .ok_or(Error::DeviceDisconnected)?
                .stop()
                .await
                .map_err(map_rtlsdr_error)?;
            self.cleanup_required = false;
        }
        self.active = false;
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
        if buffers[0].is_empty() {
            return Ok(0);
        }

        let out = &mut buffers[0];
        let stream = self.stream.as_mut().ok_or(Error::DeviceDisconnected)?;
        let read = match with_timeout(
            stream.read(out, None).into_future(),
            timeout_from_micros(timeout_us),
        )
        .await
        {
            TimeoutResult::Completed(read) => read.map_err(map_rtlsdr_error)?,
            TimeoutResult::TimedOut => 0,
        };
        Ok(read)
    }
}

impl Drop for AsyncRtlSdrRxStreamer {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            let result = self.abandoned_stream_slot.put(stream);
            debug_assert!(
                result.is_ok(),
                "abandoned stream slot was unexpectedly occupied"
            );
        }
        self.active = false;
    }
}

impl AsyncTypedDeviceBackend for AsyncRtlSdr {
    fn driver() -> Driver {
        Driver::RtlSdr
    }

    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(args: &Args) -> Result<Vec<WebUsbDeviceFilter>, Error> {
        webusb_filters(args)
    }

    async fn async_probe(args: &Args) -> Result<Vec<Args>, Error> {
        Self::probe(args).await
    }

    async fn async_open(args: &Args) -> Result<Self, Error> {
        Self::open(args.clone()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "smol", feature = "tokio"))]
    #[test]
    fn dropping_async_slot_lease_returns_value() {
        let slot = Shared::new(AsyncSlot::new(7));

        let lease = AsyncSlotLease::acquire(&slot).expect("acquire slot lease");
        assert!(matches!(AsyncSlotLease::acquire(&slot), Err(Error::Busy)));
        drop(lease);

        let lease = AsyncSlotLease::acquire(&slot).expect("reacquire slot lease");
        assert_eq!(*lease.value.as_ref().expect("lease owns value"), 7);
    }
}
