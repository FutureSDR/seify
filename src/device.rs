#![allow(dead_code)]
#![allow(unused_variables)]
use std::any::Any;
use std::sync::Arc;

use crate::impls;
use crate::Args;
use crate::Direction;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RxStreamer;
use crate::TxStreamer;

pub trait DeviceTrait: Any + Send {
    type RxStreamer: RxStreamer;
    type TxStreamer: TxStreamer;

    /// Cast to Any for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Cast to Any for downcasting to a mutable reference.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// SDR driver
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
    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, Error>;
    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::RxStreamer, Error>;
    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, Error>;
    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::TxStreamer, Error>;

    //================================ ANTENNA ============================================
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error>;
    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error>;
    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error>;

    /// List available amplification elements.
    ///
    /// Elements should be in order RF to baseband.
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error>;

    //================================ AGC ============================================
    /// Does the device support automatic gain control?
    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error>;

    /// Enable or disable automatic gain control.
    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error>;

    /// Returns true if automatic gain control is enabled
    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error>;

    //================================ GAIN ============================================
    /// Set the overall amplification in a chain.
    ///
    /// The gain will be distributed automatically across available elements.
    ///
    /// `gain`: the new amplification value in dB
    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error>;

    /// Get the overall value of the gain elements in a chain in dB.
    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error>;

    /// Get the overall range of possible gain values.
    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;

    /// Set the value of a amplification element in a chain.
    ///
    /// # Arguments
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
    ///
    /// Recommended names used to represent tunable components:
    ///
    ///   - "CORR" - freq error correction in PPM
    ///   - "RF" - frequency of the RF frontend
    ///   - "BB" - frequency of the baseband DSP
    ///
    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error>;

    //================================ SAMPLE RATE ============================================

    /// Get the baseband sample rate of the chain in samples per second.
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error>;

    /// Set the baseband sample rate of the chain in samples per second.
    fn set_sample_rate(&self, direction: Direction, channel: usize, rate: f64)
        -> Result<(), Error>;

    /// Get the range of possible baseband sample rates.
    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error>;
}

pub struct Device<T: DeviceTrait + Clone + Any> {
    dev: T,
}

// impl<T> Clone for Device<T>
// where
//     T: DeviceTrait + Any,
//     T: Clone, {
//     fn clone(&self) -> Self {
//         Self { dev: self.dev.clone() }
//     }
// }

impl Device<GenericDevice> {
    pub fn new() -> Result<Self, Error> {
        let mut devs = crate::enumerate()?;
        if devs.is_empty() {
            return Err(Error::NotFound);
        }
        Self::from_args(devs.remove(0))
    }

    pub fn from_args<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args.try_into().or(Err(Error::ValueError))?;
        let driver = match args.get::<Driver>("driver") {
            Ok(d) => Some(d),
            Err(Error::NotFound) => None,
            Err(e) => return Err(e),
        };
        #[cfg(feature = "aaronia")]
        {
            if driver.is_none() || matches!(driver, Some(Driver::Aaronia)) {
                return Ok(Device {
                    dev: Arc::new(DeviceWrapper {
                        dev: impls::Aaronia::open(&args)?,
                    }),
                });
            }
        }
        #[cfg(feature = "rtlsdr")]
        {
            if driver.is_none() || matches!(driver, Some(Driver::RtlSdr)) {
                return Ok(Device {
                    dev: Arc::new(DeviceWrapper {
                        dev: impls::RtlSdr::open(&args)?,
                    }),
                });
            }
        }
        #[cfg(feature = "hackrf")]
        {
            if driver.is_none() || matches!(driver, Some(Driver::HackRf)) {
                return Ok(Device {
                    dev: Arc::new(Box::new(impls::HackRf::open(&args)?)),
                });
            }
        }
        Err(Error::NotFound)
    }
}

pub type GenericDevice =
    Arc<dyn DeviceTrait<RxStreamer = Box<dyn RxStreamer>, TxStreamer = Box<dyn TxStreamer>> + Sync>;

impl<T: DeviceTrait + Clone + Any> Device<T> {
    pub fn from_device(dev: T) -> Self {
        Self { dev }
    }
    pub fn inner<D: DeviceTrait + Any>(&self) -> Result<&D, Error> {
        if let Some(d) = self.dev.as_any().downcast_ref::<D>() {
            return Ok(d);
        }
        let d = self
            .dev
            .as_any()
            .downcast_ref::<Box<
                (dyn DeviceTrait<
                    RxStreamer = Box<(dyn RxStreamer + 'static)>,
                    TxStreamer = Box<(dyn TxStreamer + 'static)>,
                > + 'static),
            >>()
            .ok_or(Error::ValueError)?;

        let d = (**d)
            .as_any()
            .downcast_ref::<DeviceWrapper<D>>()
            .ok_or(Error::ValueError)?;
        Ok(&d.dev)
    }
    pub fn inner_mut<D: DeviceTrait + Any>(&mut self) -> Result<&mut D, Error> {
        // work around borrow checker limitation
        if let Some(d) = self.dev.as_any().downcast_ref::<D>() {
            Ok(self.dev.as_any_mut().downcast_mut::<D>().unwrap())
        } else {
            let d = self
                .dev
                .as_any_mut()
                .downcast_mut::<Box<
                    (dyn DeviceTrait<
                        RxStreamer = Box<(dyn RxStreamer + 'static)>,
                        TxStreamer = Box<(dyn TxStreamer + 'static)>,
                    > + 'static),
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

    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, Error> {
        Ok(Box::new(self.dev.rx_stream(channels)?))
    }

    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        Ok(Box::new(self.dev.rx_stream_with_args(channels, args)?))
    }

    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, Error> {
        Ok(Box::new(self.dev.tx_stream(channels)?))
    }

    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        Ok(Box::new(self.dev.tx_stream_with_args(channels, args)?))
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

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.dev.gain_elements(direction, channel)
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.suports_agc(direction, channel)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        self.dev.enable_agc(direction, channel, agc)
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.agc(direction, channel)
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
    ) -> Result<(), Error> {
        self.dev.set_frequency(direction, channel, frequency)
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
        args: Args,
    ) -> Result<(), Error> {
        self.dev
            .set_component_frequency(direction, channel, name, frequency, args)
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
}

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

    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, Error> {
        Ok(Box::new(self.as_ref().rx_stream(channels)?))
    }

    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        Ok(Box::new(self.as_ref().rx_stream_with_args(channels, args)?))
    }

    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, Error> {
        Ok(Box::new(self.as_ref().tx_stream(channels)?))
    }

    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        Ok(Box::new(self.as_ref().tx_stream_with_args(channels, args)?))
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

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.as_ref().gain_elements(direction, channel)
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().suports_agc(direction, channel)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        self.as_ref().enable_agc(direction, channel, agc)
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.as_ref().agc(direction, channel)
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
    ) -> Result<(), Error> {
        self.as_ref().set_frequency(direction, channel, frequency)
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
        args: Args,
    ) -> Result<(), Error> {
        self.as_ref()
            .set_component_frequency(direction, channel, name, frequency, args)
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
}

impl<
        R: RxStreamer + 'static,
        T: TxStreamer + 'static,
        D: DeviceTrait<RxStreamer = R, TxStreamer = T> + Clone + 'static,
    > Device<D>
{
    pub fn driver(&self) -> Driver {
        self.dev.driver()
    }
    pub fn id(&self) -> Result<String, Error> {
        self.dev.id()
    }
    pub fn info(&self) -> Result<Args, Error> {
        self.dev.info()
    }
    pub fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        self.dev.num_channels(direction)
    }
    pub fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.full_duplex(direction, channel)
    }

    pub fn rx_stream(&self, channels: &[usize]) -> Result<R, Error> {
        self.dev.rx_stream(channels)
    }

    pub fn rx_stream_with_args(&self, channels: &[usize], args: Args) -> Result<R, Error> {
        self.dev.rx_stream_with_args(channels, args)
    }

    pub fn tx_stream(&self, channels: &[usize]) -> Result<T, Error> {
        self.dev.tx_stream(channels)
    }

    pub fn tx_stream_with_args(&self, channels: &[usize], args: Args) -> Result<T, Error> {
        self.dev.tx_stream_with_args(channels, args)
    }

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

    pub fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        self.dev.gain_elements(direction, channel)
    }

    pub fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.suports_agc(direction, channel)
    }

    pub fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        self.dev.enable_agc(direction, channel, agc)
    }

    pub fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        self.dev.agc(direction, channel)
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
        self.dev.set_frequency(direction, channel, frequency)
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
        args: Args,
    ) -> Result<(), Error> {
        self.dev
            .set_component_frequency(direction, channel, name, frequency, args)
    }

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
