#![allow(dead_code)]
#![allow(unused_variables)]
use std::any::Any;
use std::sync::Arc;

use crate::Args;
use crate::Direction;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RxStreamer;
use crate::TxStreamer;

/// Central trait, implemented by hardware drivers.
pub trait DeviceTrait: Any + Send {
    /// Associated RX streamer
    type RxStreamer: RxStreamer;
    /// Associated TX streamer
    type TxStreamer: TxStreamer;

    /// Cast to Any for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Cast to Any for downcasting to a mutable reference.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// SDR [driver](Driver)
    fn driver(&self) -> Driver;
    /// Identifier for the device, e.g., its serial.
    fn id(&self) -> Result<String, Error>;
    /// Device info that can be displayed to the user.
    fn info(&self) -> Result<Args, Error>;
    /// Number of supported Channels.
    fn num_channels(&self, direction: Direction) -> Result<usize, Error>;
    /// Full Duplex support.
    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error>;

    //================================ STREAMER ============================================
    /// Create an RX streamer.
    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error>;
    /// Create a TX streamer.
    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error>;

    //================================ ANTENNA ============================================
    /// List of available antenna ports.
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error>;
    /// Currently used antenna port.
    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error>;
    /// Set antenna port.
    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error>;

    //================================ AGC ============================================
    /// Does the device support automatic gain control?
    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error>;

    /// Enable or disable automatic gain control.
    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error>;

    /// Returns true, if automatic gain control is enabled
    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error>;

    //================================ GAIN ============================================
    /// List of available gain elements.
    ///
    /// Elements should be in order RF to baseband.
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error>;
    /// Set the overall amplification in a chain.
    ///
    /// The gain will be distributed automatically across available elements.
    ///
    /// `gain`: the new amplification value in dB
    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error>;

    /// Get the overall value of the gain elements in a chain in dB.
    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error>;

    /// Get the overall [`Range`] of possible gain values.
    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;

    /// Set the value of an amplification element in a chain.
    ///
    /// ## Arguments
    /// * `name`: the name of an amplification element from `Device::list_gains`
    /// * `gain`: the new amplification value in dB
    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error>;

    /// Get the value of an individual amplification element in a chain in dB.
    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error>;

    /// Get the range of possible gain values for a specific element.
    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error>;

    //================================ FREQUENCY ============================================

    /// Get the ranges of overall frequency values.
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;

    /// Get the overall center frequency of the chain.
    ///
    ///   - For RX, this specifies the down-conversion frequency.
    ///   - For TX, this specifies the up-conversion frequency.
    ///
    /// Returns the center frequency in Hz.
    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error>;

    /// Set the center frequency of the chain.
    ///
    ///   - For RX, this specifies the down-conversion frequency.
    ///   - For TX, this specifies the up-conversion frequency.
    ///
    /// The default implementation of `set_frequency` will tune the "RF"
    /// component as close as possible to the requested center frequency in Hz.
    /// Tuning inaccuracies will be compensated for with the "BB" component.
    ///
    /// The `args` can be used to augment the tuning algorithm.
    ///
    ///   - Use `"OFFSET"` to specify an "RF" tuning offset,
    ///     usually with the intention of moving the LO out of the passband.
    ///     The offset will be compensated for using the "BB" component.
    ///   - Use the name of a component for the key and a frequency in Hz
    ///     as the value (any format) to enforce a specific frequency.
    ///     The other components will be tuned with compensation
    ///     to achieve the specified overall frequency.
    ///   - Use the name of a component for the key and the value `"IGNORE"`
    ///     so that the tuning algorithm will avoid altering the component.
    ///   - Vendor specific implementations can also use the same args to augment
    ///     tuning in other ways such as specifying fractional vs integer N tuning.
    ///
    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error>;

    /// List available tunable elements in the chain.
    ///
    /// Elements should be in order RF to baseband.
    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error>;

    /// Get the range of tunable values for the specified element.
    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error>;

    /// Get the frequency of a tunable element in the chain.
    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error>;

    /// Tune the center frequency of the specified element.
    ///
    ///   - For RX, this specifies the down-conversion frequency.
    ///   - For TX, this specifies the up-conversion frequency.
    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error>;

    //================================ SAMPLE RATE ============================================

    /// Get the baseband sample rate of the chain in samples per second.
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error>;

    /// Set the baseband sample rate of the chain in samples per second.
    fn set_sample_rate(&self, direction: Direction, channel: usize, rate: f64)
        -> Result<(), Error>;

    /// Get the range of possible baseband sample rates.
    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;

    //================================ BANDWIDTH ============================================

    /// Get the hardware bandwidth filter, if available.
    ///
    /// Returns `Err(Error::NotSupported)` if unsupported in underlying driver.
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error>;

    /// Set the hardware bandwidth filter, if available.
    ///
    /// Returns `Err(Error::NotSupported)` if unsupported in underlying driver.
    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error>;

    /// Get the range of possible bandwidth filter values, if available.
    ///
    /// Returns `Err(Error::NotSupported)` if unsupported in underlying driver.
    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;

    //========================= AUTOMATIC DC OFFSET CORRECTIONS ===============================

    /// Returns true if automatic corrections are supported
    fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error>;

    /// Enable or disable automatic DC offset corrections mode.
    fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error>;

    /// Returns true if automatic DC offset mode is enabled
    fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error>;
}

/// Wrapps a driver, implementing the [DeviceTrait].
///
/// Implements a more ergonomic version of the [`DeviceTrait`], e.g., using `Into<Args>`, which
/// would not be possible in traits.
#[derive(Clone)]
pub struct Device<T: DeviceTrait + Clone> {
    dev: T,
}

impl Device<GenericDevice> {
    /// Creates a [`GenericDevice`] opening the first device discovered through
    /// [`enumerate`](crate::enumerate).
    pub fn new() -> Result<Self, Error> {
        let mut devs = crate::enumerate()?;
        if devs.is_empty() {
            return Err(Error::NotFound);
        }
        Self::from_args(devs.remove(0))
    }

    /// Create a generic device from a device implementation.
    pub fn generic_from_impl<T: DeviceTrait + Clone + Sync>(dev: T) -> Self {
        Self {
            dev: Arc::new(DeviceWrapper { dev }),
        }
    }

    /// Creates a [`GenericDevice`] opening the first device with a given `driver`, specified in
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
                    Ok(d) => {
                        return Ok(Device {
                            dev: Arc::new(DeviceWrapper { dev: d }),
                        })
                    }
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
                    Ok(d) => {
                        return Ok(Device {
                            dev: Arc::new(DeviceWrapper { dev: d }),
                        })
                    }
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
                    Ok(d) => {
                        return Ok(Device {
                            dev: Arc::new(DeviceWrapper { dev: d }),
                        })
                    }
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
                    Ok(d) => {
                        return Ok(Device {
                            dev: Arc::new(DeviceWrapper { dev: d }),
                        })
                    }
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
                    Ok(d) => {
                        return Ok(Device {
                            dev: Arc::new(DeviceWrapper { dev: d }),
                        })
                    }
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
                    Ok(d) => {
                        return Ok(Device {
                            dev: Arc::new(DeviceWrapper { dev: d }),
                        })
                    }
                    Err(Error::NotFound) => {
                        if driver.is_some() {
                            return Err(Error::NotFound);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(feature = "dummy")]
        {
            if driver.is_none() || matches!(driver, Some(Driver::Dummy)) {
                match crate::impls::Dummy::open(&args) {
                    Ok(d) => {
                        return Ok(Device {
                            dev: Arc::new(DeviceWrapper { dev: d }),
                        })
                    }
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

/// Type for a generic/wrapped hardware driver, implementing the [`DeviceTrait`].
///
/// This is usually used to create a hardware-independent `Device<GenericDevice>`, for example,
/// through [`Device::new`], which doesn't know a priori which implementation will be used.
/// The type abstracts over the `DeviceTrait` implementation as well as the associated
/// streamer implementations.
pub type GenericDevice =
    Arc<dyn DeviceTrait<RxStreamer = Box<dyn RxStreamer>, TxStreamer = Box<dyn TxStreamer>> + Sync>;

impl<T: DeviceTrait + Clone> Device<T> {
    /// Create a device from the device implementation.
    pub fn from_impl(dev: T) -> Self {
        Self { dev }
    }
    /// Try to downcast to a given device implementation `D`, either directly (from `Device<D>`)
    /// or indirectly (from a `Device<GenericDevice>` that wraps a `D`).
    pub fn impl_ref<D: DeviceTrait>(&self) -> Result<&D, Error> {
        if let Some(d) = self.dev.as_any().downcast_ref::<D>() {
            return Ok(d);
        }

        let d = self
            .dev
            .as_any()
            .downcast_ref::<Arc<
                dyn DeviceTrait<
                        RxStreamer = Box<dyn RxStreamer + 'static>,
                        TxStreamer = Box<dyn TxStreamer + 'static>,
                    > + Sync
                    + 'static,
            >>()
            .ok_or(Error::ValueError)?;

        let d = (**d)
            .as_any()
            .downcast_ref::<DeviceWrapper<D>>()
            .ok_or(Error::ValueError)?;
        Ok(&d.dev)
    }
    /// Try to downcast mutably to a given device implementation `D`, either directly
    /// (from `Device<D>`) or indirectly (from a `Device<GenericDevice>` that wraps a `D`).
    pub fn impl_mut<D: DeviceTrait>(&mut self) -> Result<&mut D, Error> {
        // work around borrow checker limitation
        if let Some(d) = self.dev.as_any().downcast_ref::<D>() {
            Ok(self.dev.as_any_mut().downcast_mut::<D>().unwrap())
        } else {
            let d = self
                .dev
                .as_any_mut()
                .downcast_mut::<Box<
                    dyn DeviceTrait<
                            RxStreamer = Box<dyn RxStreamer + 'static>,
                            TxStreamer = Box<dyn TxStreamer + 'static>,
                        > + 'static,
                >>()
                .ok_or(Error::ValueError)?;

            let d = (**d)
                .as_any_mut()
                .downcast_mut::<DeviceWrapper<D>>()
                .ok_or(Error::ValueError)?;
            Ok(&mut d.dev)
        }
    }
}

struct DeviceWrapper<D: DeviceTrait> {
    dev: D,
}

impl<
        R: RxStreamer + 'static,
        T: TxStreamer + 'static,
        D: DeviceTrait<RxStreamer = R, TxStreamer = T>,
    > DeviceTrait for DeviceWrapper<D>
{
    type RxStreamer = Box<dyn RxStreamer>;
    type TxStreamer = Box<dyn TxStreamer>;

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn driver(&self) -> Driver {
        self.dev.driver()
    }
    fn id(&self) -> Result<String, Error> {
        self.dev.id()
    }
    fn info(&self) -> Result<Args, Error> {
        self.dev.info()
    }
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        self.dev.num_channels(direction)
    }
    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.full_duplex(direction, channel)
    }

    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error> {
        Ok(Box::new(self.dev.rx_streamer(channels, args)?))
    }
    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error> {
        Ok(Box::new(self.dev.tx_streamer(channels, args)?))
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.dev.antennas(direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.dev.antenna(direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        self.dev.set_antenna(direction, channel, name)
    }

    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.supports_agc(direction, channel)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        self.dev.enable_agc(direction, channel, agc)
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.agc(direction, channel)
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.dev.gain_elements(direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.dev.set_gain(direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.dev.gain(direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.dev.gain_range(direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        self.dev.set_gain_element(direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        self.dev.gain_element(direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.dev.gain_element_range(direction, channel, name)
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.dev.frequency_range(direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.frequency(direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        self.dev.set_frequency(direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.dev.frequency_components(direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.dev.component_frequency_range(direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.dev.component_frequency(direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        self.dev
            .set_component_frequency(direction, channel, name, frequency)
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.sample_rate(direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.dev.set_sample_rate(direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.dev.get_sample_rate_range(direction, channel)
    }

    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.bandwidth(direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        self.dev.set_bandwidth(direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.dev.get_bandwidth_range(direction, channel)
    }

    fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.has_dc_offset_mode(direction, channel)
    }

    fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error> {
        self.dev.set_dc_offset_mode(direction, channel, automatic)
    }

    fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.dc_offset_mode(direction, channel)
    }
}

#[doc(hidden)]
impl DeviceTrait for GenericDevice {
    type RxStreamer = Box<dyn RxStreamer>;
    type TxStreamer = Box<dyn TxStreamer>;

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
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        self.as_ref().num_channels(direction)
    }
    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().full_duplex(direction, channel)
    }

    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error> {
        Ok(Box::new(self.as_ref().rx_streamer(channels, args)?))
    }

    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error> {
        Ok(Box::new(self.as_ref().tx_streamer(channels, args)?))
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.as_ref().antennas(direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.as_ref().antenna(direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        self.as_ref().set_antenna(direction, channel, name)
    }

    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().supports_agc(direction, channel)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        self.as_ref().enable_agc(direction, channel, agc)
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().agc(direction, channel)
    }

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

    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.as_ref().bandwidth(direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        self.as_ref().set_bandwidth(direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.as_ref().get_bandwidth_range(direction, channel)
    }

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

impl<
        R: RxStreamer + 'static,
        T: TxStreamer + 'static,
        D: DeviceTrait<RxStreamer = R, TxStreamer = T> + Clone + 'static,
    > Device<D>
{
    /// SDR [driver](Driver)
    pub fn driver(&self) -> Driver {
        self.dev.driver()
    }
    /// Identifier for the device, e.g., its serial.
    pub fn id(&self) -> Result<String, Error> {
        self.dev.id()
    }
    /// Device info that can be displayed to the user.
    pub fn info(&self) -> Result<Args, Error> {
        self.dev.info()
    }
    /// Number of supported Channels.
    pub fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        self.dev.num_channels(direction)
    }
    /// Full Duplex support.
    pub fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.full_duplex(direction, channel)
    }

    //================================ STREAMER ============================================
    /// Create an RX streamer.
    pub fn rx_streamer(&self, channels: &[usize]) -> Result<R, Error> {
        self.dev.rx_streamer(channels, Args::new())
    }
    /// Create an RX streamer, using `args`.
    pub fn rx_streamer_with_args(&self, channels: &[usize], args: Args) -> Result<R, Error> {
        self.dev.rx_streamer(channels, args)
    }
    /// Create a TX Streamer.
    pub fn tx_streamer(&self, channels: &[usize]) -> Result<T, Error> {
        self.dev.tx_streamer(channels, Args::new())
    }
    /// Create a TX Streamer, using `args`.
    pub fn tx_streamer_with_args(&self, channels: &[usize], args: Args) -> Result<T, Error> {
        self.dev.tx_streamer(channels, args)
    }

    //================================ ANTENNA ============================================
    /// List of available antenna ports.
    pub fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.dev.antennas(direction, channel)
    }
    /// Currently used antenna port.
    pub fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        self.dev.antenna(direction, channel)
    }
    /// Set antenna port.
    pub fn set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        self.dev.set_antenna(direction, channel, name)
    }

    //================================ AGC ============================================
    /// Does the device support automatic gain control?
    pub fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.supports_agc(direction, channel)
    }
    /// Enable or disable automatic gain control.
    pub fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        self.dev.enable_agc(direction, channel, agc)
    }
    /// Returns true, if automatic gain control is enabled
    pub fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.agc(direction, channel)
    }

    //================================ GAIN ============================================
    /// List of available gain elements.
    ///
    /// Elements should be in order RF to baseband.
    pub fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.dev.gain_elements(direction, channel)
    }

    /// Set the overall amplification in a chain.
    ///
    /// The gain will be distributed automatically across available elements.
    ///
    /// `gain`: the new amplification value in dB
    pub fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.dev.set_gain(direction, channel, gain)
    }

    /// Get the overall value of the gain elements in a chain in dB.
    pub fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.dev.gain(direction, channel)
    }

    /// Get the overall [`Range`] of possible gain values.
    pub fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.dev.gain_range(direction, channel)
    }

    /// Set the value of an amplification element in a chain.
    ///
    /// ## Arguments
    /// * `name`: the name of an amplification element from `Device::list_gains`
    /// * `gain`: the new amplification value in dB
    pub fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        self.dev.set_gain_element(direction, channel, name, gain)
    }

    /// Get the value of an individual amplification element in a chain in dB.
    pub fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        self.dev.gain_element(direction, channel, name)
    }

    /// Get the range of possible gain values for a specific element.
    pub fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.dev.gain_element_range(direction, channel, name)
    }

    //================================ FREQUENCY ============================================

    /// Get the ranges of overall frequency values.
    pub fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.dev.frequency_range(direction, channel)
    }

    /// Get the overall center frequency of the chain.
    ///
    ///   - For RX, this specifies the down-conversion frequency.
    ///   - For TX, this specifies the up-conversion frequency.
    ///
    /// Returns the center frequency in Hz.
    pub fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.frequency(direction, channel)
    }

    /// Set the center frequency of the chain.
    ///
    ///   - For RX, this specifies the down-conversion frequency.
    ///   - For TX, this specifies the up-conversion frequency.
    ///
    /// The default implementation of `set_frequency` will tune the "RF"
    /// component as close as possible to the requested center frequency in Hz.
    /// Tuning inaccuracies will be compensated for with the "BB" component.
    ///
    pub fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
    ) -> Result<(), Error> {
        self.dev
            .set_frequency(direction, channel, frequency, Args::new())
    }

    /// Like [`set_frequency`](Self::set_frequency) but using `args` to augment the tuning algorithm.
    ///
    ///   - Use `"OFFSET"` to specify an "RF" tuning offset,
    ///     usually with the intention of moving the LO out of the passband.
    ///     The offset will be compensated for using the "BB" component.
    ///   - Use the name of a component for the key and a frequency in Hz
    ///     as the value (any format) to enforce a specific frequency.
    ///     The other components will be tuned with compensation
    ///     to achieve the specified overall frequency.
    ///   - Use the name of a component for the key and the value `"IGNORE"`
    ///     so that the tuning algorithm will avoid altering the component.
    ///   - Vendor specific implementations can also use the same args to augment
    ///     tuning in other ways such as specifying fractional vs integer N tuning.
    ///
    pub fn set_frequency_with_args(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        self.dev.set_frequency(direction, channel, frequency, args)
    }

    /// List available tunable elements in the chain.
    ///
    /// Elements should be in order RF to baseband.
    pub fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.dev.frequency_components(direction, channel)
    }

    /// Get the range of tunable values for the specified element.
    pub fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        self.dev.component_frequency_range(direction, channel, name)
    }

    /// Get the frequency of a tunable element in the chain.
    pub fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.dev.component_frequency(direction, channel, name)
    }

    /// Tune the center frequency of the specified element.
    ///
    ///   - For RX, this specifies the down-conversion frequency.
    ///   - For TX, this specifies the up-conversion frequency.
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

    //================================ SAMPLE RATE ============================================

    /// Get the baseband sample rate of the chain in samples per second.
    pub fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.sample_rate(direction, channel)
    }

    /// Set the baseband sample rate of the chain in samples per second.
    pub fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.dev.set_sample_rate(direction, channel, rate)
    }

    /// Get the range of possible baseband sample rates.
    pub fn get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        self.dev.get_sample_rate_range(direction, channel)
    }

    //================================ BANDWIDTH ============================================

    /// Get the hardware bandwidth filter, if available.
    ///
    /// Returns `Err(Error::NotSupported)` if unsupported in underlying driver.
    pub fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.dev.bandwidth(direction, channel)
    }

    /// Set the hardware bandwidth filter, if available.
    ///
    /// Returns `Err(Error::NotSupported)` if unsupported in underlying driver.
    pub fn set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bw: f64,
    ) -> Result<(), Error> {
        self.dev.set_bandwidth(direction, channel, bw)
    }

    /// Get the range of possible bandwidth filter values, if available.
    ///
    /// Returns `Err(Error::NotSupported)` if unsupported in underlying driver.
    pub fn get_bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Range, Error> {
        self.dev.get_bandwidth_range(direction, channel)
    }

    //========================= AUTOMATIC DC OFFSET CORRECTIONS ===============================
    /// Returns true if automatic corrections are supported
    pub fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.has_dc_offset_mode(direction, channel)
    }

    /// Set the automatic DC offset correction mode
    pub fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error> {
        self.dev.set_dc_offset_mode(direction, channel, automatic)
    }

    /// Returns true if automatic DC offset mode is enabled
    pub fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.dc_offset_mode(direction, channel)
    }
}
