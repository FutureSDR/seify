use hydrasdr_rs::{DecimationPolicy, Device as HydraSdrDevice, RxStream};
use num_complex::Complex32;
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

/// Asynchronous HydraSDR RFOne device backend.
#[derive(Clone)]
pub struct AsyncHydraSdr {
    device_slot: Shared<AsyncSlot<Box<HydraSdrDevice>>>,
    abandoned_stream_slot: Shared<AsyncSlot<RxStream>>,
    serial: u64,
    inner: Shared<ReceiverContext>,
}

/// HydraSDR RFOne asynchronous receive streamer.
///
/// The streamer shares ownership of the hardware with the device, which remains
/// available for control operations while reception is active. Dropping a
/// streamer defers final close to the next asynchronous device operation.
#[must_use = "deactivate the HydraSDR stream before dropping it"]
pub struct AsyncHydraSdrRxStreamer {
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
        .map_err(map_hydrasdr_error)?;
    Ok(())
}

impl AsyncHydraSdr {
    /// Return descriptors for detected HydraSDR RFOne devices asynchronously.
    ///
    /// Every descriptor contains a normalized serial. Zero represents a
    /// missing, invalid, or zero USB serial and may match any such device.
    pub async fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let devices = HydraSdrDevice::list().await.map_err(map_hydrasdr_error)?;
        probe_args(args, devices)
    }

    /// Open a HydraSDR RFOne device from arguments asynchronously.
    pub async fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = device_selector(&args)?;
        let (dev, serial) = open_selected_device_async(selector).await?;
        let sample_rates = dev.sample_rates();
        let receiver_context = ReceiverContext::from_device_info(dev.info(), sample_rates);

        Ok(Self {
            device_slot: Shared::new(AsyncSlot::new(Box::new(dev))),
            abandoned_stream_slot: Shared::new(AsyncSlot::empty()),
            serial,
            inner: Shared::new(receiver_context),
        })
    }

    async fn lease_device(&self) -> Result<AsyncSlotLease<Box<HydraSdrDevice>>, Error> {
        let device = AsyncSlotLease::acquire(&self.device_slot)?;
        cleanup_abandoned_stream(&self.abandoned_stream_slot).await?;
        Ok(device)
    }
}

impl AsyncHydraSdr {
    fn driver(&self) -> Driver {
        Driver::HydraSdr
    }

    async fn id(&self) -> Result<String, Error> {
        Ok(self.serial.to_string())
    }

    async fn info(&self) -> Result<Args, Error> {
        let mut args = Args::default();
        args.set("driver", "hydrasdr");
        args.set("serial", self.id().await?);
        Ok(args)
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
        let device = self.lease_device().await?;
        self.inner.antenna(device.value().config())
    }

    async fn set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let port = self
            .inner
            .rf_port_for_antenna(name)
            .ok_or(Error::invalid_argument(
                "antenna",
                "antenna is not available on this HydraSDR device",
            ))?;
        let mut device = self.lease_device().await?;
        device
            .value_mut()
            .set_rf_port(port)
            .await
            .map_err(map_hydrasdr_error)
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
            .map_err(map_hydrasdr_error)
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
            .map_err(map_hydrasdr_error)
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
            "hydrasdr",
            "invalid HydraSDR argument",
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
            .map_err(map_hydrasdr_error)
    }

    async fn gain_element(
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
                "hydrasdr",
                "invalid HydraSDR argument",
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
                "hydrasdr",
                "invalid HydraSDR argument",
            ));
        }
        let device = self.lease_device().await?;
        self.inner.frequency(device.value().config())
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
            .map_err(map_hydrasdr_error)
    }

    async fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        let device = self.lease_device().await?;
        self.inner.sample_rate(device.value().config())
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
            .map_err(map_hydrasdr_error)
    }

    async fn get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        sample_rate_range(&self.inner.sample_rates)
    }
}

impl AsyncDeviceInfo for AsyncHydraSdr {
    fn driver(&self) -> Driver {
        AsyncHydraSdr::driver(self)
    }

    async fn async_id(&self) -> Result<String, Error> {
        AsyncHydraSdr::id(self).await
    }

    async fn async_info(&self) -> Result<Args, Error> {
        AsyncHydraSdr::info(self).await
    }

    async fn async_num_channels(&self, direction: Direction) -> Result<usize, Error> {
        AsyncHydraSdr::num_channels(self, direction).await
    }

    async fn async_full_duplex(&self) -> Result<bool, Error> {
        AsyncHydraSdr::full_duplex(self).await
    }
}

crate::impl_dyn_async_device_backend!(
    AsyncHydraSdr => [rx, antenna, agc, gain, frequency, sample_rate]
);

impl AsyncRxDevice for AsyncHydraSdr {
    type RxStreamer = AsyncHydraSdrRxStreamer;

    async fn async_rx_streamer(
        &self,
        channels: &[usize],
        _args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::invalid_argument(
                "hydrasdr",
                "invalid HydraSDR argument",
            ));
        }
        let device = self.lease_device().await?;
        let stream = device.value().rx_stream().map_err(map_hydrasdr_error)?;
        Ok(AsyncHydraSdrRxStreamer::new(
            Shared::clone(&self.abandoned_stream_slot),
            stream,
        ))
    }
}

impl AsyncAntennaControl for AsyncHydraSdr {
    async fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncHydraSdr::antennas(self, direction, channel).await
    }

    async fn async_antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        AsyncHydraSdr::antenna(self, direction, channel).await
    }

    async fn async_set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        AsyncHydraSdr::set_antenna(self, direction, channel, name).await
    }
}

impl AsyncAgcControl for AsyncHydraSdr {
    async fn async_agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<bool, Error> {
        AsyncHydraSdr::agc_available(self, direction, channel).await
    }

    async fn async_agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        AsyncHydraSdr::agc_enabled(self, direction, channel).await
    }

    async fn async_set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        AsyncHydraSdr::set_agc_enabled(self, direction, channel, enabled).await
    }
}

impl AsyncGainControl for AsyncHydraSdr {
    async fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncHydraSdr::gain_elements(self, direction, channel).await
    }

    async fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        AsyncHydraSdr::set_gain(self, direction, channel, gain).await
    }

    async fn async_gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        AsyncHydraSdr::gain(self, direction, channel).await
    }

    async fn async_gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        AsyncHydraSdr::gain_range(self, direction, channel).await
    }

    async fn async_set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        AsyncHydraSdr::set_gain_element(self, direction, channel, name, gain).await
    }

    async fn async_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        AsyncHydraSdr::gain_element(self, direction, channel, name).await
    }

    async fn async_gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AsyncHydraSdr::gain_element_range(self, direction, channel, name).await
    }
}

impl AsyncFrequencyControl for AsyncHydraSdr {
    async fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncHydraSdr::frequency_range(self, direction, channel).await
    }

    async fn async_frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncHydraSdr::frequency(self, direction, channel).await
    }

    async fn async_set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        AsyncHydraSdr::set_frequency(self, direction, channel, frequency, args).await
    }

    async fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncHydraSdr::frequency_components(self, direction, channel).await
    }

    async fn async_component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AsyncHydraSdr::component_frequency_range(self, direction, channel, name).await
    }

    async fn async_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        AsyncHydraSdr::component_frequency(self, direction, channel, name).await
    }

    async fn async_set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        AsyncHydraSdr::set_component_frequency(self, direction, channel, name, frequency).await
    }
}

impl AsyncSampleRateControl for AsyncHydraSdr {
    async fn async_sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncHydraSdr::sample_rate(self, direction, channel).await
    }

    async fn async_set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        AsyncHydraSdr::set_sample_rate(self, direction, channel, rate).await
    }

    async fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncHydraSdr::get_sample_rate_range(self, direction, channel).await
    }
}

impl AsyncHydraSdrRxStreamer {
    fn new(abandoned_stream_slot: Shared<AsyncSlot<RxStream>>, stream: RxStream) -> Self {
        Self {
            abandoned_stream_slot,
            stream: Some(stream),
            active: false,
            cleanup_required: false,
        }
    }
}

impl crate::AsyncRxStreamer for AsyncHydraSdrRxStreamer {
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
            .map_err(map_hydrasdr_error)?;
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
                .map_err(map_hydrasdr_error)?;
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
            TimeoutResult::Completed(read) => read.map_err(map_hydrasdr_error)?,
            TimeoutResult::TimedOut => 0,
        };
        Ok(read)
    }
}

impl Drop for AsyncHydraSdrRxStreamer {
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

impl AsyncTypedDeviceBackend for AsyncHydraSdr {
    fn driver() -> Driver {
        Driver::HydraSdr
    }

    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(args: &Args) -> Result<Vec<WebUsbDeviceFilter>, Error> {
        let serial = match device_selector(args)? {
            DeviceSelector::Serial(serial) if serial != 0 => {
                Some(format!("HYDRASDR SN:{serial:016X}"))
            }
            DeviceSelector::First | DeviceSelector::Serial(_) | DeviceSelector::Index(_) => None,
        };
        Ok([(0x1d50, 0x60a1), (0x38af, 0x0001)]
            .into_iter()
            .map(|(vendor_id, product_id)| {
                let filter = WebUsbDeviceFilter::new().with_vendor_product(vendor_id, product_id);
                if let Some(serial) = &serial {
                    filter.with_serial_number(serial.clone())
                } else {
                    filter
                }
            })
            .collect())
    }

    async fn async_probe(args: &Args) -> Result<Vec<Args>, Error> {
        Self::probe(args).await
    }

    async fn async_open(args: &Args) -> Result<Self, Error> {
        Self::open(args.clone()).await
    }
}

async fn open_selected_device_async(
    selector: DeviceSelector,
) -> Result<(HydraSdrDevice, u64), Error> {
    match selector {
        DeviceSelector::First => HydraSdrDevice::builder()
            .decimation_policy(DecimationPolicy::HighDefinition)
            .open()
            .await
            .map(|dev| {
                let serial = dev.info().serial.normalized();
                (dev, serial)
            })
            .map_err(map_hydrasdr_error),
        DeviceSelector::Serial(serial) => HydraSdrDevice::builder()
            .serial(serial)
            .decimation_policy(DecimationPolicy::HighDefinition)
            .open()
            .await
            .map(|dev| (dev, serial))
            .map_err(map_hydrasdr_error),
        DeviceSelector::Index(index) => {
            let devices = HydraSdrDevice::list().await.map_err(map_hydrasdr_error)?;
            let Some(info) = devices.get(index) else {
                return Err(Error::DeviceNotFound);
            };
            let serial = info.serial.normalized();
            HydraSdrDevice::builder()
                .serial(serial)
                .decimation_policy(DecimationPolicy::HighDefinition)
                .open()
                .await
                .map(|dev| (dev, serial))
                .map_err(map_hydrasdr_error)
        }
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
