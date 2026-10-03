use libbladerf_rs::bladerf1::{
    BladeRf1, ExpansionBoard, GainMode, RfLinkSession, RxStream, SampleFormat, TuningMode, TxStream,
};
use libbladerf_rs::Channel;
use num_complex::Complex32;
use std::future::{Future, IntoFuture};
use std::ops::AsyncFnOnce;

use super::common::*;
use super::convert::*;
use super::selector::*;
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
}

/// bladeRF 1 asynchronous transmit streamer.
#[must_use = "deactivate the bladeRF stream before dropping it"]
pub struct AsyncBladeRfTxStreamer {
    device_slot: Shared<AsyncSlot<Box<BladeRf1>>>,
    abandoned: Shared<AsyncSlot<TxStream>>,
    stream: Option<TxStream>,
    format: SampleFormat,
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

/// Opens a fresh [`RfLinkSession`] over `lease` and runs `operation` with it.
///
/// The caller supplies the lease, so both entry points share one path:
///
/// - [`AsyncBladeRf::with_session`] passes [`AsyncBladeRf::lease_device`], which
///   first reaps streams parked in the abandoned slots by dropped streamers.
/// - the streamers pass an `async` block around `AsyncSlotLease::acquire`,
///   which only takes the slot, because a streamer must not reap other streams
///   while it is starting or stopping its own.
async fn with_lease<T>(
    lease: impl Future<Output = Result<DeviceLease, Error>>,
    operation: impl AsyncFnOnce(&mut RfLinkSession<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let mut dev = lease.await?;
    let mut rf = session(&mut dev).await?;
    operation(&mut rf).await
}

async fn cleanup_abandoned(
    dev: &mut DeviceLease,
    rx: &Shared<AsyncSlot<RxStream>>,
    tx: &Shared<AsyncSlot<TxStream>>,
) -> Result<(), Error> {
    if let Ok(mut stream) = AsyncSlotLease::acquire(rx) {
        let mut rf = session(dev).await?;
        stream
            .value_mut()
            .close(&mut rf)
            .await
            .map_err(bladerf_err)?;
        stream.value = None;
    }
    if let Ok(mut stream) = AsyncSlotLease::acquire(tx) {
        let mut rf = session(dev).await?;
        stream
            .value_mut()
            .close(&mut rf)
            .await
            .map_err(bladerf_err)?;
        stream.value = None;
    }
    Ok(())
}

impl AsyncBladeRf {
    /// Return descriptors for detected bladeRF 1 devices asynchronously.
    ///
    /// On WebUSB only devices the page has already been granted are listed;
    /// call `AsyncRegistry::request_permission`
    /// from a user gesture first.
    #[cfg(not(target_os = "android"))]
    pub async fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let selector = device_selector(args)?;
        let descriptors = BladeRf1::list_bladerf1()
            .await
            .map_err(|_| Error::DeviceNotFound)?
            .map(|info| probe_descriptor(&info))
            .collect();
        Ok(filter_descriptors(&selector, descriptors))
    }

    /// Returns no descriptors on Android, which requires [`Self::from_fd`].
    #[cfg(target_os = "android")]
    pub async fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        Ok(Vec::new())
    }

    /// Open a bladeRF 1 device from arguments asynchronously.
    #[cfg(not(target_os = "android"))]
    pub async fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args: Args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        Self::init_and_wrap(open_selected_device(device_selector(&args)?).await?).await
    }

    /// Reports that Android opening requires [`Self::from_fd`].
    ///
    /// # Errors
    /// Returns an unsupported-operation error because Android cannot enumerate USB devices.
    #[cfg(target_os = "android")]
    pub async fn open<A: TryInto<Args>>(_args: A) -> Result<Self, Error> {
        Err(Error::unsupported_reason(
            Capability::DriverOperation,
            "Android requires AsyncBladeRf::from_fd with an owned USB connection",
        ))
    }

    /// Opens a bladeRF 1 asynchronously from an owned USB file descriptor.
    ///
    /// Android applications obtain USB permission before calling this constructor.
    /// Duplicate the descriptor first if Java retains its connection ownership.
    ///
    /// # Examples
    /// ```no_run
    /// # async fn open(fd: std::os::fd::OwnedFd) -> Result<seify::DynAsyncDevice, seify::Error> {
    /// let backend = seify::impls::AsyncBladeRf::from_fd(fd).await?;
    /// Ok(seify::DynAsyncDevice::from_impl(backend))
    /// # }
    /// ```
    ///
    /// # Errors
    /// Propagates USB opening, interface claiming, and initialization failures.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub async fn from_fd(fd: std::os::fd::OwnedFd) -> Result<Self, Error> {
        Self::init_and_wrap(BladeRf1::from_fd(fd).await.map_err(bladerf_err)?).await
    }

    async fn init_and_wrap(mut dev: BladeRf1) -> Result<Self, Error> {
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

    /// Leases the device and runs `operation` with a fresh [`RfLinkSession`].
    ///
    /// The lease reaps streams abandoned by dropped streamers before the
    /// session is handed out; see [`with_lease`].
    async fn with_session<T>(
        &self,
        operation: impl AsyncFnOnce(&mut RfLinkSession<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        with_lease(self.lease_device(), operation).await
    }
}

#[cfg(not(target_os = "android"))]
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

    async fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(ch(direction, channel)? == Channel::Rx)
    }

    async fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        let channel = ch(direction, channel)?;
        if channel != Channel::Rx {
            return Err(Error::unsupported(Capability::Agc));
        }
        let mode = if agc {
            GainMode::Default
        } else {
            GainMode::Mgc
        };
        self.with_session(async |rf| rf.set_gain_mode(channel, mode).await.map_err(bladerf_err))
            .await
    }

    async fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        if ch(direction, channel)? != Channel::Rx {
            return Err(Error::unsupported(Capability::Agc));
        }
        self.with_session(async |rf| {
            Ok(rf.get_gain_mode().await.map_err(bladerf_err)? == GainMode::Default)
        })
        .await
    }

    async fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        Ok(RfLinkSession::get_gain_stages(ch(direction, channel)?)
            .iter()
            .map(|s| <&str>::from(*s).to_string())
            .collect())
    }

    async fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        let channel = ch(direction, channel)?;
        let gain = clamp_gain(RfLinkSession::get_gain_range(channel), gain);
        self.with_session(async |rf| rf.set_gain(channel, gain).await.map_err(bladerf_err))
            .await
    }

    async fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.with_session(async |rf| {
            Ok(Some(
                rf.get_gain(ch(direction, channel)?)
                    .await
                    .map_err(bladerf_err)?
                    .db() as f64,
            ))
        })
        .await
    }

    async fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        RfLinkSession::get_gain_range(ch(direction, channel)?).try_into()
    }

    async fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let stage = gain_stage(direction, channel, name)?;
        let gain = clamp_gain(RfLinkSession::get_gain_stage_range(stage), gain);
        self.with_session(async |rf| rf.set_gain_stage(stage, gain).await.map_err(bladerf_err))
            .await
    }

    async fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        let stage = gain_stage(direction, channel, name)?;
        self.with_session(async |rf| {
            Ok(Some(
                rf.get_gain_stage(stage).await.map_err(bladerf_err)?.db() as f64,
            ))
        })
        .await
    }

    async fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let stage = gain_stage(direction, channel, name)?;
        RfLinkSession::get_gain_stage_range(stage).try_into()
    }

    async fn frequency_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        self.with_session(async |rf| {
            rf.get_frequency_range()
                .await
                .map_err(bladerf_err)?
                .try_into()
        })
        .await
    }

    async fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.with_session(async |rf| {
            Ok(rf
                .get_frequency(ch(direction, channel)?)
                .await
                .map_err(bladerf_err)? as f64)
        })
        .await
    }

    async fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        self.with_session(async |rf| {
            let f_range = rf.get_frequency_range().await.map_err(bladerf_err)?;
            if needs_xb200(frequency, f_range) {
                log::trace!("Frequency {frequency} requires XB200 expansion board");
                if rf.expansion_get_attached().await.map_err(bladerf_err)? != ExpansionBoard::Xb200
                {
                    log::debug!("Automatically attaching XB200 expansion board");
                    rf.expansion_attach(ExpansionBoard::Xb200)
                        .await
                        .map_err(bladerf_err)?;
                }
            }
            log::trace!("Setting frequency to {frequency}");
            let ch = ch(direction, channel)?;
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
        })
        .await
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

    async fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.with_session(async |rf| {
            Ok(rf
                .get_sample_rate(ch(direction, channel)?)
                .await
                .map_err(bladerf_err)? as f64)
        })
        .await
    }

    async fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.with_session(async |rf| {
            let ch = ch(direction, channel)?;
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
        })
        .await
    }

    async fn get_sample_rate_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        RfLinkSession::get_sample_rate_range().try_into()
    }

    async fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.with_session(async |rf| {
            Ok(rf
                .get_bandwidth(ch(direction, channel)?)
                .await
                .map_err(bladerf_err)? as f64)
        })
        .await
    }

    async fn set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bw: f64,
    ) -> Result<(), Error> {
        self.with_session(async |rf| {
            let actual = rf
                .set_bandwidth(ch(direction, channel)?, bw as u32)
                .await
                .map_err(bladerf_err)?;
            if actual != bw as u32 {
                log::debug!("Requested bandwidth {bw}, actual {actual}");
            }
            Ok(())
        })
        .await
    }

    async fn get_bandwidth_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        RfLinkSession::get_bandwidth_range().try_into()
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
        let stream = self
            .with_session(async |rf| {
                RxStream::builder(rf)
                    .buffer_size(BUFFER_SIZE)
                    .buffer_count(BUFFER_COUNT)
                    .format(STREAM_FORMAT)
                    .build()
                    .await
                    .map_err(bladerf_err)
            })
            .await?;
        Ok(AsyncBladeRfRxStreamer {
            device_slot: Shared::clone(&self.device_slot),
            abandoned: Shared::clone(&self.abandoned_rx),
            stream: Some(stream),
            converter: RxConverter::new(STREAM_FORMAT),
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
        let stream = self
            .with_session(async |rf| {
                TxStream::builder(rf)
                    .buffer_size(BUFFER_SIZE)
                    .buffer_count(BUFFER_COUNT)
                    .format(STREAM_FORMAT)
                    .build()
                    .await
                    .map_err(bladerf_err)
            })
            .await?;
        Ok(AsyncBladeRfTxStreamer {
            device_slot: Shared::clone(&self.device_slot),
            abandoned: Shared::clone(&self.abandoned_tx),
            stream: Some(stream),
            format: STREAM_FORMAT,
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
    fn mtu(
        &self,
    ) -> impl std::future::Future<Output = Result<usize, Error>> + crate::MaybeSend + '_ {
        std::future::ready(Ok(BUFFER_SIZE / STREAM_FORMAT.sample_size()))
    }

    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        let stream = self.stream.as_mut().ok_or(Error::StreamInactive)?;
        with_lease(
            async { AsyncSlotLease::acquire(&self.device_slot) },
            async |rf| match stream.start(rf).await {
                Ok(()) | Err(libbladerf_rs::Error::StreamAlreadyStarted) => Ok(()),
                Err(error) => Err(bladerf_err(error)),
            },
        )
        .await
    }

    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        let stream = self.stream.as_mut().ok_or(Error::StreamInactive)?;
        if let Some(buffer) = self.converter.take_pending() {
            stream.recycle(buffer);
        }
        with_lease(
            async { AsyncSlotLease::acquire(&self.device_slot) },
            async |rf| match stream.stop(rf).await {
                Ok(()) | Err(libbladerf_rs::Error::StreamNotStarted) => Ok(()),
                Err(error) => Err(bladerf_err(error)),
            },
        )
        .await
    }

    async fn read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> Result<usize, Error> {
        check_buffer_count(buffers.len())?;
        let out = &mut buffers[0];
        if out.is_empty() {
            return Ok(0);
        }
        let stream = self.stream.as_mut().ok_or(Error::StreamInactive)?;

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
        if let Some(mut stream) = self.stream.take() {
            if let Some(buffer) = self.converter.take_pending() {
                stream.recycle(buffer);
            }
            let result = self.abandoned.put(stream);
            debug_assert!(
                result.is_ok(),
                "abandoned RX slot was unexpectedly occupied"
            );
        }
    }
}

impl crate::AsyncTxStreamer for AsyncBladeRfTxStreamer {
    fn mtu(
        &self,
    ) -> impl std::future::Future<Output = Result<usize, Error>> + crate::MaybeSend + '_ {
        let samples = BUFFER_SIZE / self.format.sample_size();
        std::future::ready(Ok(samples))
    }

    async fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedActivation));
        }
        let stream = self.stream.as_mut().ok_or(Error::StreamInactive)?;
        with_lease(
            async { AsyncSlotLease::acquire(&self.device_slot) },
            async |rf| match stream.start(rf).await {
                Ok(()) | Err(libbladerf_rs::Error::StreamAlreadyStarted) => Ok(()),
                Err(error) => Err(bladerf_err(error)),
            },
        )
        .await
    }

    async fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::unsupported(Capability::TimedDeactivation));
        }
        let stream = self.stream.as_mut().ok_or(Error::StreamInactive)?;
        with_lease(
            async { AsyncSlotLease::acquire(&self.device_slot) },
            async |rf| match stream.stop(rf).await {
                Ok(()) | Err(libbladerf_rs::Error::StreamNotStarted) => Ok(()),
                Err(error) => Err(bladerf_err(error)),
            },
        )
        .await
    }

    async fn write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        check_buffer_count(buffers.len())?;
        if buffers[0].is_empty() {
            return Ok(0);
        }
        let stream = self.stream.as_mut().ok_or(Error::StreamInactive)?;
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
