use libbladerf_rs::bladerf1::hardware::lms6002d::gain::GainStage;
use libbladerf_rs::bladerf1::{
    BladeRf1, ExpansionBoard, GainDb, GainMode, RfLinkSession, RxStream, SampleFormat, TuningMode,
    TxStream,
};
use num_complex::Complex32;
use std::future::IntoFuture;

use super::common::*;
#[cfg(target_arch = "wasm32")]
use crate::dev::WebUsbDeviceFilter;
use crate::{
    async_compat::{timeout_from_micros, with_timeout, Shared, TimeoutResult},
    dev::AsyncTypedDeviceBackend,
    Args, AsyncAgcControl, AsyncAntennaControl, AsyncBandwidthControl, AsyncDeviceInfo,
    AsyncFrequencyControl, AsyncGainControl, AsyncRxDevice, AsyncSampleRateControl, AsyncTxDevice,
    Capability, Direction, Driver, DriverError, Error, Range,
};

/// Asynchronous bladeRF 1 device backend.
#[derive(Clone)]
pub struct AsyncBladeRf {
    device_slot: Shared<AsyncSlot<Box<BladeRf1>>>,
    abandoned_rx: Shared<AsyncSlot<RxStream>>,
    abandoned_tx: Shared<AsyncSlot<TxStream>>,
    serial: String,
}

/// bladeRF 1 asynchronous receive streamer.
///
/// The streamer owns the USB bulk queue; the device is only leased for
/// `activate`/`deactivate`. Dropping a streamer without deactivating it
/// defers stream teardown to the next asynchronous device operation.
#[must_use = "deactivate the bladeRF stream before dropping it"]
pub struct AsyncBladeRfRxStreamer {
    device_slot: Shared<AsyncSlot<Box<BladeRf1>>>,
    abandoned: Shared<AsyncSlot<RxStream>>,
    stream: Option<RxStream>,
    converter: RxConverter,
    active: bool,
}

/// bladeRF 1 asynchronous transmit streamer.
#[must_use = "deactivate the bladeRF stream before dropping it"]
pub struct AsyncBladeRfTxStreamer {
    device_slot: Shared<AsyncSlot<Box<BladeRf1>>>,
    abandoned: Shared<AsyncSlot<TxStream>>,
    stream: Option<TxStream>,
    format: SampleFormat,
    active: bool,
}

#[cfg(not(target_arch = "wasm32"))]
struct AsyncSlot<T>(std::sync::Mutex<Option<T>>);

#[cfg(target_arch = "wasm32")]
struct AsyncSlot<T>(std::cell::RefCell<Option<T>>);

impl<T> AsyncSlot<T> {
    fn new(value: T) -> Self {
        let slot = Self::empty();
        let result = slot.put(value);
        debug_assert!(result.is_ok());
        slot
    }

    fn empty() -> Self {
        Self(Default::default())
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

    fn value_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("slot lease always owns a value")
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

type DeviceLease = AsyncSlotLease<Box<BladeRf1>>;

async fn session(dev: &mut DeviceLease) -> Result<RfLinkSession<'_>, Error> {
    dev.value_mut().rf_link_session().await.map_err(bladerf_err)
}

async fn cleanup_abandoned(
    dev: &mut DeviceLease,
    rx: &Shared<AsyncSlot<RxStream>>,
    tx: &Shared<AsyncSlot<TxStream>>,
) -> Result<(), Error> {
    if let Some(mut stream) = rx.take() {
        let mut rf = session(dev).await?;
        stream.close(&mut rf).await.map_err(bladerf_err)?;
    }
    if let Some(mut stream) = tx.take() {
        let mut rf = session(dev).await?;
        stream.close(&mut rf).await.map_err(bladerf_err)?;
    }
    Ok(())
}

impl AsyncBladeRf {
    /// Return descriptors for detected bladeRF 1 devices asynchronously.
    ///
    /// On WebUSB only devices the page has already been granted are listed;
    /// call [`AsyncRegistry::request_permission`](crate::AsyncRegistry::request_permission)
    /// from a user gesture first.
    pub async fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let selector = device_selector(args)?;
        let descriptors = BladeRf1::list_bladerf1()
            .await
            .map_err(|_| Error::DeviceNotFound)?
            .map(|info| probe_descriptor(&info))
            .collect();
        Ok(filter_descriptors(&selector, descriptors))
    }

    /// Open a bladeRF 1 device from arguments asynchronously.
    pub async fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args: Args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let mut dev = open_selected_device(device_selector(&args)?).await?;
        dev.rf_link_session()
            .await
            .map_err(bladerf_err)?
            .initialize(false)
            .await
            .map_err(bladerf_err)?;
        let serial = dev.serial().await.map_err(bladerf_err)?;
        Ok(Self {
            device_slot: Shared::new(AsyncSlot::new(Box::new(dev))),
            abandoned_rx: Shared::new(AsyncSlot::empty()),
            abandoned_tx: Shared::new(AsyncSlot::empty()),
            serial,
        })
    }

    async fn lease_device(&self) -> Result<DeviceLease, Error> {
        let mut dev = AsyncSlotLease::acquire(&self.device_slot)?;
        cleanup_abandoned(&mut dev, &self.abandoned_rx, &self.abandoned_tx).await?;
        Ok(dev)
    }
}

async fn open_selected_device(selector: DeviceSelector) -> Result<BladeRf1, Error> {
    match selector {
        #[cfg(target_os = "linux")]
        DeviceSelector::Fd(fd) => {
            use std::os::fd::{FromRawFd, OwnedFd};
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
            BladeRf1::from_fd(fd).await
        }
        #[cfg(not(target_arch = "wasm32"))]
        DeviceSelector::BusAddr(bus_id, address) => BladeRf1::from_bus_addr(&bus_id, address).await,
        DeviceSelector::Serial(serial) => BladeRf1::from_serial(&serial).await,
        DeviceSelector::First => BladeRf1::from_first().await,
    }
    .map_err(bladerf_err)
}

impl AsyncBladeRf {
    fn driver(&self) -> Driver {
        Driver::BladeRf
    }

    async fn id(&self) -> Result<String, Error> {
        Ok(self.serial.clone())
    }

    async fn info(&self) -> Result<Args, Error> {
        let mut dev = self.lease_device().await?;
        let firmware = dev
            .value_mut()
            .fx3_firmware_version()
            .await
            .map_err(bladerf_err)?;
        let mut args = Args::default();
        args.set("driver", "bladerf");
        args.set("serial", self.serial.clone());
        args.set("firmware version", firmware);
        Ok(args)
    }

    async fn num_channels(&self, _direction: Direction) -> Result<usize, Error> {
        Ok(1)
    }

    async fn full_duplex(&self) -> Result<bool, Error> {
        Ok(true)
    }

    async fn antennas(&self, _direction: Direction, _channel: usize) -> Result<Vec<String>, Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    async fn antenna(&self, _direction: Direction, _channel: usize) -> Result<String, Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    async fn set_antenna(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<(), Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    async fn agc_available(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
        let mut dev = self.lease_device().await?;
        let rf = session(&mut dev).await?;
        Ok(rf.get_gain_modes(ch(channel)?).is_ok())
    }

    async fn set_agc_enabled(
        &self,
        _direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        let mode = if agc {
            GainMode::Default
        } else {
            GainMode::Mgc
        };
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        rf.set_gain_mode(ch(channel)?, mode)
            .await
            .map_err(bladerf_err)
    }

    async fn agc_enabled(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        Ok(rf.get_gain_mode().await.is_ok())
    }

    async fn gain_elements(
        &self,
        _direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        Ok(RfLinkSession::get_gain_stages(ch(channel)?)
            .iter()
            .map(|s| <&str>::from(*s).to_string())
            .collect())
    }

    async fn set_gain(
        &self,
        _direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        let range = RfLinkSession::get_gain_range(ch(channel)?);
        let min = range.min().unwrap_or(f64::MIN);
        let max = range.max().unwrap_or(f64::MAX);
        let clamped = gain.clamp(min, max);
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        rf.set_gain(ch(channel)?, GainDb::from(clamped as i8))
            .await
            .map_err(bladerf_err)
    }

    async fn gain(&self, _direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        Ok(Some(
            rf.get_gain(ch(channel)?).await.map_err(bladerf_err)?.db() as f64,
        ))
    }

    async fn gain_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        Ok(RfLinkSession::get_gain_range(ch(channel)?).into())
    }

    async fn set_gain_element(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let stage = GainStage::try_from(name).map_err(|_| invalid_argument())?;
        let range = RfLinkSession::get_gain_stage_range(stage);
        let min = range.min().unwrap_or(f64::MIN);
        let max = range.max().unwrap_or(f64::MAX);
        let clamped = gain.clamp(min, max);
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        rf.set_gain_stage(stage, GainDb::from(clamped as i8))
            .await
            .map_err(bladerf_err)
    }

    async fn gain_element(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        let stage = GainStage::try_from(name).map_err(|_| invalid_argument())?;
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        Ok(Some(
            rf.get_gain_stage(stage).await.map_err(bladerf_err)?.db() as f64,
        ))
    }

    async fn gain_element_range(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let stage = GainStage::try_from(name).map_err(|_| invalid_argument())?;
        Ok(RfLinkSession::get_gain_stage_range(stage).into())
    }

    async fn frequency_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        Ok(rf.get_frequency_range().await.map_err(bladerf_err)?.into())
    }

    async fn frequency(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        Ok(rf.get_frequency(ch(channel)?).await.map_err(bladerf_err)? as f64)
    }

    async fn set_frequency(
        &self,
        _direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        let ch = ch(channel)?;
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        let f_range = rf.get_frequency_range().await.map_err(bladerf_err)?;
        if frequency < f_range.min().unwrap_or(f64::MIN) {
            log::trace!("Frequency {frequency} requires XB200 expansion board");
            if rf.expansion_get_attached().await.map_err(bladerf_err)? != ExpansionBoard::Xb200 {
                log::debug!("Automatically attaching XB200 expansion board");
                rf.expansion_attach(ExpansionBoard::Xb200)
                    .await
                    .map_err(bladerf_err)?;
            }
        }
        log::trace!("Setting frequency to {frequency}");
        if rf
            .set_frequency(ch, frequency as u64, TuningMode::Fpga)
            .await
            .is_err()
        {
            log::warn!("FPGA retune failed, falling back to host tuning");
            rf.set_frequency(ch, frequency as u64, TuningMode::Host)
                .await
                .map_err(bladerf_err)?;
        }
        Ok(())
    }

    async fn frequency_components(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Vec<String>, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    async fn component_frequency_range(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<Range, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    async fn component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<f64, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    async fn set_component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
        _frequency: f64,
    ) -> Result<(), Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    async fn sample_rate(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        Ok(rf
            .get_sample_rate(ch(channel)?)
            .await
            .map_err(bladerf_err)? as f64)
    }

    async fn set_sample_rate(
        &self,
        _direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let ch = ch(channel)?;
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        let actual = rf
            .set_sample_rate(ch, rate as u32)
            .await
            .map_err(bladerf_err)?;
        if actual != rate as u32 {
            log::debug!("Requested sample rate {rate}, actual {actual}");
        }
        let bw_actual = rf.set_bandwidth(ch, actual).await.map_err(bladerf_err)?;
        if bw_actual != actual {
            log::debug!("Auto-set bandwidth to {bw_actual} (requested {actual})");
        }
        Ok(())
    }

    async fn get_sample_rate_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        Ok(RfLinkSession::get_sample_rate_range().into())
    }

    async fn bandwidth(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        Ok(rf.get_bandwidth(ch(channel)?).await.map_err(bladerf_err)? as f64)
    }

    async fn set_bandwidth(
        &self,
        _direction: Direction,
        channel: usize,
        bw: f64,
    ) -> Result<(), Error> {
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        let actual = rf
            .set_bandwidth(ch(channel)?, bw as u32)
            .await
            .map_err(bladerf_err)?;
        if actual != bw as u32 {
            log::debug!("Requested bandwidth {bw}, actual {actual}");
        }
        Ok(())
    }

    async fn get_bandwidth_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        Ok(RfLinkSession::get_bandwidth_range().into())
    }
}

impl AsyncDeviceInfo for AsyncBladeRf {
    fn driver(&self) -> Driver {
        AsyncBladeRf::driver(self)
    }

    async fn async_id(&self) -> Result<String, Error> {
        AsyncBladeRf::id(self).await
    }

    async fn async_info(&self) -> Result<Args, Error> {
        AsyncBladeRf::info(self).await
    }

    async fn async_num_channels(&self, direction: Direction) -> Result<usize, Error> {
        AsyncBladeRf::num_channels(self, direction).await
    }

    async fn async_full_duplex(&self) -> Result<bool, Error> {
        AsyncBladeRf::full_duplex(self).await
    }
}

crate::impl_dyn_async_device_backend!(
    AsyncBladeRf => [rx, tx, antenna, agc, gain, frequency, sample_rate, bandwidth]
);

impl AsyncRxDevice for AsyncBladeRf {
    type RxStreamer = AsyncBladeRfRxStreamer;

    async fn async_rx_streamer(
        &self,
        channels: &[usize],
        _args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        check_channels(channels, "RX")?;
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        let stream = RxStream::builder(&mut rf)
            .buffer_size(BUFFER_SIZE)
            .buffer_count(BUFFER_COUNT)
            .format(STREAM_FORMAT)
            .build()
            .await
            .map_err(bladerf_err)?;
        Ok(AsyncBladeRfRxStreamer {
            device_slot: Shared::clone(&self.device_slot),
            abandoned: Shared::clone(&self.abandoned_rx),
            stream: Some(stream),
            converter: RxConverter::new(STREAM_FORMAT),
            active: false,
        })
    }
}

impl AsyncTxDevice for AsyncBladeRf {
    type TxStreamer = AsyncBladeRfTxStreamer;

    async fn async_tx_streamer(
        &self,
        channels: &[usize],
        _args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        check_channels(channels, "TX")?;
        let mut dev = self.lease_device().await?;
        let mut rf = session(&mut dev).await?;
        let stream = TxStream::builder(&mut rf)
            .buffer_size(BUFFER_SIZE)
            .buffer_count(BUFFER_COUNT)
            .format(STREAM_FORMAT)
            .build()
            .await
            .map_err(bladerf_err)?;
        Ok(AsyncBladeRfTxStreamer {
            device_slot: Shared::clone(&self.device_slot),
            abandoned: Shared::clone(&self.abandoned_tx),
            stream: Some(stream),
            format: STREAM_FORMAT,
            active: false,
        })
    }
}

impl AsyncAntennaControl for AsyncBladeRf {
    async fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncBladeRf::antennas(self, direction, channel).await
    }

    async fn async_antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        AsyncBladeRf::antenna(self, direction, channel).await
    }

    async fn async_set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        AsyncBladeRf::set_antenna(self, direction, channel, name).await
    }
}

impl AsyncAgcControl for AsyncBladeRf {
    async fn async_agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<bool, Error> {
        AsyncBladeRf::agc_available(self, direction, channel).await
    }

    async fn async_agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        AsyncBladeRf::agc_enabled(self, direction, channel).await
    }

    async fn async_set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> Result<(), Error> {
        AsyncBladeRf::set_agc_enabled(self, direction, channel, enabled).await
    }
}

impl AsyncGainControl for AsyncBladeRf {
    async fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncBladeRf::gain_elements(self, direction, channel).await
    }

    async fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        AsyncBladeRf::set_gain(self, direction, channel, gain).await
    }

    async fn async_gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        AsyncBladeRf::gain(self, direction, channel).await
    }

    async fn async_gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        AsyncBladeRf::gain_range(self, direction, channel).await
    }

    async fn async_set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        AsyncBladeRf::set_gain_element(self, direction, channel, name, gain).await
    }

    async fn async_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        AsyncBladeRf::gain_element(self, direction, channel, name).await
    }

    async fn async_gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AsyncBladeRf::gain_element_range(self, direction, channel, name).await
    }
}

impl AsyncFrequencyControl for AsyncBladeRf {
    async fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncBladeRf::frequency_range(self, direction, channel).await
    }

    async fn async_frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncBladeRf::frequency(self, direction, channel).await
    }

    async fn async_set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        AsyncBladeRf::set_frequency(self, direction, channel, frequency, args).await
    }

    async fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AsyncBladeRf::frequency_components(self, direction, channel).await
    }

    async fn async_component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AsyncBladeRf::component_frequency_range(self, direction, channel, name).await
    }

    async fn async_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        AsyncBladeRf::component_frequency(self, direction, channel, name).await
    }

    async fn async_set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        AsyncBladeRf::set_component_frequency(self, direction, channel, name, frequency).await
    }
}

impl AsyncSampleRateControl for AsyncBladeRf {
    async fn async_sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncBladeRf::sample_rate(self, direction, channel).await
    }

    async fn async_set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        AsyncBladeRf::set_sample_rate(self, direction, channel, rate).await
    }

    async fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncBladeRf::get_sample_rate_range(self, direction, channel).await
    }
}

impl AsyncBandwidthControl for AsyncBladeRf {
    async fn async_bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AsyncBladeRf::bandwidth(self, direction, channel).await
    }

    async fn async_set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bw: f64,
    ) -> Result<(), Error> {
        AsyncBladeRf::set_bandwidth(self, direction, channel, bw).await
    }

    async fn async_get_bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        AsyncBladeRf::get_bandwidth_range(self, direction, channel).await
    }
}

impl crate::AsyncRxStreamer for AsyncBladeRfRxStreamer {
    async fn mtu(&self) -> Result<usize, Error> {
        Ok(BUFFER_SIZE / STREAM_FORMAT.sample_size())
    }

    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        if self.active {
            return Ok(());
        }
        let stream = self.stream.as_mut().ok_or(Error::DeviceDisconnected)?;
        let mut dev = AsyncSlotLease::acquire(&self.device_slot)?;
        let mut rf = session(&mut dev).await?;
        stream.start(&mut rf).await.map_err(bladerf_err)?;
        self.active = true;
        Ok(())
    }

    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        if !self.active {
            return Ok(());
        }
        let stream = self.stream.as_mut().ok_or(Error::DeviceDisconnected)?;
        let mut dev = AsyncSlotLease::acquire(&self.device_slot)?;
        let mut rf = session(&mut dev).await?;
        self.active = false;
        stream.stop(&mut rf).await.map_err(bladerf_err)
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
        let out = &mut buffers[0];
        if out.is_empty() {
            return Ok(0);
        }
        let stream = self.stream.as_mut().ok_or(Error::DeviceDisconnected)?;

        let (mut written, recycled) = self.converter.drain_pending(out)?;
        if let Some(buf) = recycled {
            stream.recycle(buf);
        }
        if written >= out.len() {
            return Ok(written);
        }

        let dma_buffer = match with_timeout(
            stream.read(None).into_future(),
            timeout_from_micros(timeout_us),
        )
        .await
        {
            TimeoutResult::Completed(buffer) => buffer.map_err(bladerf_err)?,
            TimeoutResult::TimedOut => return Ok(written),
        };
        let (n, recycled) = self.converter.consume(dma_buffer, &mut out[written..])?;
        if let Some(buf) = recycled {
            stream.recycle(buf);
        }
        written += n;
        Ok(written)
    }
}

impl Drop for AsyncBladeRfRxStreamer {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            let result = self.abandoned.put(stream);
            debug_assert!(
                result.is_ok(),
                "abandoned RX slot was unexpectedly occupied"
            );
        }
        self.active = false;
    }
}

impl crate::AsyncTxStreamer for AsyncBladeRfTxStreamer {
    async fn mtu(&self) -> Result<usize, Error> {
        Ok(BUFFER_SIZE / self.format.sample_size())
    }

    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        if self.active {
            return Ok(());
        }
        let stream = self.stream.as_mut().ok_or(Error::DeviceDisconnected)?;
        let mut dev = AsyncSlotLease::acquire(&self.device_slot)?;
        let mut rf = session(&mut dev).await?;
        stream.start(&mut rf).await.map_err(bladerf_err)?;
        self.active = true;
        Ok(())
    }

    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        if !self.active {
            return Ok(());
        }
        let stream = self.stream.as_mut().ok_or(Error::DeviceDisconnected)?;
        let mut dev = AsyncSlotLease::acquire(&self.device_slot)?;
        let mut rf = session(&mut dev).await?;
        self.active = false;
        stream.stop(&mut rf).await.map_err(bladerf_err)
    }

    async fn write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::StreamInactive);
        }
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        if buffers[0].is_empty() {
            return Ok(0);
        }
        let stream = self.stream.as_mut().ok_or(Error::DeviceDisconnected)?;
        let bytes_per_sample = self.format.sample_size();
        let samples_to_write = buffers[0].len().min(BUFFER_SIZE / bytes_per_sample);
        let bytes_needed = samples_to_write * bytes_per_sample;

        let mut dma_buffer = match with_timeout(
            stream.get_buffer(None).into_future(),
            timeout_from_micros(timeout_us),
        )
        .await
        {
            TimeoutResult::Completed(buffer) => buffer.map_err(bladerf_err)?,
            TimeoutResult::TimedOut => return Err(Error::Timeout),
        };
        dma_buffer.clear();
        let converted = convert_complex32_to_bytes(
            self.format,
            &buffers[0][..samples_to_write],
            dma_buffer.extend_fill(bytes_needed, 0),
        )?;
        if converted != samples_to_write {
            stream.recycle(dma_buffer);
            return Err(Error::Driver(DriverError::Other(
                "sample conversion produced a short TX buffer".into(),
            )));
        }
        stream
            .submit(dma_buffer, bytes_needed)
            .map_err(bladerf_err)?;
        Ok(samples_to_write)
    }
}

impl Drop for AsyncBladeRfTxStreamer {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            let result = self.abandoned.put(stream);
            debug_assert!(
                result.is_ok(),
                "abandoned TX slot was unexpectedly occupied"
            );
        }
        self.active = false;
    }
}

impl AsyncTypedDeviceBackend for AsyncBladeRf {
    fn driver() -> Driver {
        Driver::BladeRf
    }

    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(args: &Args) -> Result<Vec<WebUsbDeviceFilter>, Error> {
        let filter = WebUsbDeviceFilter::new().with_vendor_product(USB_VID, USB_PID);
        Ok(vec![match device_selector(args)? {
            DeviceSelector::Serial(serial) => filter.with_serial_number(serial),
            DeviceSelector::First => filter,
        }])
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
