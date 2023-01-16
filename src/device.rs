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

pub trait DeviceTrait {
    type RxStreamer: RxStreamer;
    type TxStreamer: TxStreamer;

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
    fn gain(&self, direction: Direction, channel: usize) -> Result<f64, Error>;

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
    fn gain_element(&self, direction: Direction, channel: usize, name: &str) -> Result<f64, Error>;

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
    fn list_frequencies(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error>;

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

pub struct Device<T: DeviceTrait + Any + ?Sized> {
    dev: Arc<T>,
}

impl Device<dyn DeviceTrait<RxStreamer = Box<dyn RxStreamer>, TxStreamer = Box<dyn TxStreamer>>> {
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
        #[cfg(feature = "rtlsdr")]
        {
            if driver.is_none() || matches!(driver, Some(Driver::RtlSdr)) {
                return Ok(Device {
                    dev: Arc::new(DeviceWrapper { dev: impls::RtlSdr::open(&args)?}),
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

type GenericDevice = Arc<dyn DeviceTrait<RxStreamer = Box<dyn RxStreamer>, TxStreamer = Box<dyn TxStreamer>>>;

impl<T: DeviceTrait + Any> Device<T> {
    pub fn from_device(dev: T) -> Self {
        Self { dev: Arc::new(dev) }
    }
    pub fn inner<D: Any>(&mut self) -> Result<&mut D, Error> {
        (&mut self.dev as &mut dyn Any)
            .downcast_mut::<D>()
            .ok_or(Error::ValueError)
    }
}

struct DeviceWrapper<D: DeviceTrait> {
    dev: D
}

impl<
        R: RxStreamer + 'static,
        T: TxStreamer + 'static,
        D: DeviceTrait<RxStreamer = R, TxStreamer = T>,
    > DeviceTrait for DeviceWrapper<D>
{
    type RxStreamer = Box<dyn RxStreamer>;
    type TxStreamer = Box<dyn TxStreamer>;

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
        todo!()
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        todo!()
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        todo!()
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        todo!()
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        todo!()
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        todo!()
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        todo!()
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        todo!()
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        todo!()
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn gain_element(&self, direction: Direction, channel: usize, name: &str) -> Result<f64, Error> {
        todo!()
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        todo!()
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        todo!()
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        todo!()
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        todo!()
    }

    fn list_frequencies(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        todo!()
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        todo!()
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        todo!()
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        todo!()
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        todo!()
    }
}
