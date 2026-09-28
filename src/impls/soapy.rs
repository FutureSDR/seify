//! Soapy SDR
use num_complex::Complex32;
use std::sync::OnceLock;

use crate::AgcControl;
use crate::AntennaControl;
use crate::Args;
use crate::BandwidthControl;
use crate::Capability;
use crate::DcOffsetControl;
use crate::DeviceInfo;
use crate::Direction;
use crate::Driver;
use crate::DriverError;
use crate::Error;
use crate::FrequencyControl;
use crate::GainControl;
use crate::Range;
use crate::RangeItem;
use crate::RxDevice;
use crate::SampleRateControl;
use crate::TxDevice;

/// Soapy Device
#[derive(Clone)]
pub struct Soapy {
    dev: soapysdr::Device,
    args: Args,
    index: usize,
}

/// Soapy RX Streamer
pub struct RxStreamer {
    streamer: soapysdr::RxStream<Complex32>,
}

/// Soapy TX Streamer
pub struct TxStreamer {
    streamer: soapysdr::TxStream<Complex32>,
}

/// Configures SoapySDR logging to route through the `log` crate.
///
/// This function is idempotent and will only configure logging once.
fn init_soapy_logging() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        soapysdr::configure_logging();
    });
}

impl Soapy {
    /// Get a list of detected devices, supported by Soapy
    ///
    /// The returned [`Args`] specify the device, i.e., passing them to [`Soapy::open`] will open
    /// this particular device. Using the `soapy_driver` argument it is possible to specify the
    /// `driver` argument for Soapy.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        init_soapy_logging();
        let v = soapysdr::enumerate(soapysdr::Args::try_from(args.clone())?)?;
        let v: Vec<Args> = v.into_iter().map(Into::into).collect();
        Ok(v.into_iter()
            .map(|mut a| {
                match a.get::<String>("driver") {
                    Ok(d) => {
                        a.set("soapy_driver", d);
                        a.set("driver", "soapy")
                    }
                    Err(_) => a.set("driver", "soapy"),
                };
                a
            })
            .collect())
    }
    /// Create a Soapy Device with automatic DC correction enabled where supported.
    ///
    /// It is possible to specify the Soapy `driver` argument by passing the `soapy_driver` argument
    /// to this function.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        init_soapy_logging();
        let mut args: Args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let index = args.get("index").unwrap_or(0);

        let orig_args = args.clone();
        if let Ok(d) = args.get::<String>("soapy_driver") {
            args.set("driver", d);
        } else {
            args.remove("driver");
        }

        let dev = soapysdr::Device::new(soapysdr::Args::try_from(args)?)?;
        for direction in [Direction::Rx, Direction::Tx] {
            for channel in 0..dev.num_channels(direction.into())? {
                if dev.has_dc_offset_mode(direction.into(), channel)? {
                    dev.set_dc_offset_mode(direction.into(), channel, true)?;
                }
            }
        }
        Ok(Self {
            dev,
            args: orig_args,
            index,
        })
    }
}

impl DeviceInfo for Soapy {
    fn driver(&self) -> Driver {
        Driver::Soapy
    }

    fn id(&self) -> Result<String, Error> {
        Ok(format!("{}", self.index))
    }

    fn info(&self) -> Result<Args, Error> {
        Ok(self.args.clone())
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        Ok(self.dev.num_channels(direction.into())?)
    }

    fn full_duplex(&self) -> Result<bool, Error> {
        if self.dev.num_channels(Direction::Tx.into())? == 0 {
            return Ok(false);
        }
        let channels = self.dev.num_channels(Direction::Rx.into())?;
        for channel in 0..channels {
            if self.dev.full_duplex(Direction::Rx.into(), channel)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

crate::impl_dyn_device_backend!(
    Soapy => [
        rx,
        tx,
        antenna,
        agc,
        gain,
        frequency,
        sample_rate,
        bandwidth,
        dc_offset
    ]
);
crate::registry::impl_typed_device_backend!(Soapy, Driver::Soapy);

impl RxDevice for Soapy {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::RxStreamer, Error> {
        Ok(RxStreamer {
            streamer: self
                .dev
                .rx_stream_args(channels, soapysdr::Args::try_from(args)?)?,
        })
    }
}

impl TxDevice for Soapy {
    type TxStreamer = TxStreamer;

    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error> {
        Ok(TxStreamer {
            streamer: self
                .dev
                .tx_stream_args(channels, soapysdr::Args::try_from(args)?)?,
        })
    }
}

impl AntennaControl for Soapy {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        Ok(self.dev.antennas(direction.into(), channel)?)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        Ok(self.dev.antenna(direction.into(), channel)?)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        Ok(self.dev.set_antenna(direction.into(), channel, name)?)
    }
}

impl AgcControl for Soapy {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self.dev.has_gain_mode(direction.into(), channel)?)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        Ok(self.dev.set_gain_mode(direction.into(), channel, agc)?)
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self.dev.gain_mode(direction.into(), channel)?)
    }
}

impl GainControl for Soapy {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        Ok(self.dev.list_gains(direction.into(), channel)?)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        Ok(self.dev.set_gain(direction.into(), channel, gain)?)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        if self.agc_enabled(direction, channel)? {
            Ok(None)
        } else {
            Ok(Some(self.dev.gain(direction.into(), channel)?))
        }
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let range = self.dev.gain_range(direction.into(), channel)?;
        range.try_into()
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        Ok(self
            .dev
            .set_gain_element(direction.into(), channel, name, gain)?)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        if self.agc_enabled(direction, channel)? {
            Ok(None)
        } else {
            Ok(Some(self.dev.gain_element(
                direction.into(),
                channel,
                name,
            )?))
        }
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let range = self
            .dev
            .gain_element_range(direction.into(), channel, name)?;
        range.try_into()
    }
}

impl FrequencyControl for Soapy {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let range = self.dev.frequency_range(direction.into(), channel)?;
        if range.is_empty() {
            return Err(Error::unsupported(Capability::Frequency));
        }
        range.try_into()
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        Ok(self.dev.frequency(direction.into(), channel)?)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        Ok(self.dev.set_frequency(
            direction.into(),
            channel,
            frequency,
            soapysdr::Args::try_from(args)?,
        )?)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        Ok(self.dev.list_frequencies(direction.into(), channel)?)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let range = self
            .dev
            .component_frequency_range(direction.into(), channel, name)?;
        if range.is_empty() {
            return Err(Error::unsupported(Capability::Frequency));
        }
        range.try_into()
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        Ok(self
            .dev
            .component_frequency(direction.into(), channel, name)?)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        Ok(self.dev.set_component_frequency(
            direction.into(),
            channel,
            name,
            frequency,
            soapysdr::Args::new(),
        )?)
    }
}

impl SampleRateControl for Soapy {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        Ok(self.dev.sample_rate(direction.into(), channel)?)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        Ok(self.dev.set_sample_rate(direction.into(), channel, rate)?)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let range = self.dev.get_sample_rate_range(direction.into(), channel)?;
        if range.is_empty() {
            return Err(Error::unsupported(Capability::SampleRate));
        }
        range.try_into()
    }
}

impl BandwidthControl for Soapy {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        Ok(self.dev.bandwidth(direction.into(), channel)?)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        Ok(self.dev.set_bandwidth(direction.into(), channel, bw)?)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let range = self.dev.bandwidth_range(direction.into(), channel)?;
        if range.is_empty() {
            return Err(Error::unsupported(Capability::Bandwidth));
        }
        range.try_into()
    }
}

impl DcOffsetControl for Soapy {
    fn dc_offset_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self.dev.has_dc_offset_mode(direction.into(), channel)?)
    }

    fn set_dc_offset_enabled(
        &self,
        direction: Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), Error> {
        Ok(self
            .dev
            .set_dc_offset_mode(direction.into(), channel, automatic)?)
    }

    fn dc_offset_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self.dev.dc_offset_mode(direction.into(), channel)?)
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(self.streamer.mtu()?)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        Ok(self.streamer.activate(time_ns)?)
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        Ok(self.streamer.deactivate(time_ns)?)
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        timeout_us: i64,
    ) -> Result<usize, Error> {
        Ok(self.streamer.read(buffers, timeout_us)?)
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(self.streamer.mtu()?)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        Ok(self.streamer.activate(time_ns)?)
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        Ok(self.streamer.deactivate(time_ns)?)
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        Ok(self.streamer.write(buffers, at_ns, end_burst, timeout_us)?)
    }

    fn write_all(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        Ok(self
            .streamer
            .write_all(buffers, at_ns, end_burst, timeout_us)?)
    }
}

impl From<soapysdr::Error> for Error {
    fn from(value: soapysdr::Error) -> Self {
        match value.code {
            soapysdr::ErrorCode::Timeout => Error::Timeout,
            soapysdr::ErrorCode::Overflow => Error::Overrun,
            soapysdr::ErrorCode::Underflow => Error::Underrun,
            _ => Error::Driver(DriverError::Soapy(value)),
        }
    }
}

impl From<crate::Direction> for soapysdr::Direction {
    fn from(value: crate::Direction) -> Self {
        match value {
            crate::Direction::Rx => soapysdr::Direction::Rx,
            crate::Direction::Tx => soapysdr::Direction::Tx,
        }
    }
}

impl From<soapysdr::Range> for RangeItem {
    fn from(range: soapysdr::Range) -> Self {
        if range.step == 0.0 && range.minimum == range.maximum {
            RangeItem::Value(range.minimum)
        } else if range.step == 0.0 {
            RangeItem::Interval(range.minimum, range.maximum)
        } else {
            RangeItem::Step(range.minimum, range.maximum, range.step)
        }
    }
}

impl TryFrom<soapysdr::Range> for Range {
    type Error = Error;

    fn try_from(value: soapysdr::Range) -> Result<Self, Self::Error> {
        Range::new(vec![value.into()])
    }
}

impl TryFrom<Vec<soapysdr::Range>> for Range {
    type Error = Error;

    fn try_from(value: Vec<soapysdr::Range>) -> Result<Self, Self::Error> {
        Range::new(value.into_iter().map(Into::into).collect())
    }
}

impl TryFrom<Args> for soapysdr::Args {
    type Error = Error;

    fn try_from(args: Args) -> Result<Self, Self::Error> {
        let s = format!("{args}");
        Ok(s.as_str().into())
    }
}

impl From<soapysdr::Args> for Args {
    fn from(value: soapysdr::Args) -> Self {
        let mut a = Self::new();
        for (key, value) in value.iter() {
            a.set(key, value);
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_conversion_validates_backend_data() {
        assert!(Range::try_from(Vec::<soapysdr::Range>::new()).is_err());
        for (minimum, maximum, step) in [(2.0, 1.0, 0.0), (0.0, 1.0, -1.0), (0.0, 1.0, f64::NAN)] {
            assert!(Range::try_from(soapysdr::Range {
                minimum,
                maximum,
                step
            })
            .is_err());
        }
        let range = Range::try_from(vec![
            soapysdr::Range {
                minimum: 3.0,
                maximum: 3.0,
                step: 0.0,
            },
            soapysdr::Range {
                minimum: 4.0,
                maximum: 5.0,
                step: 0.0,
            },
            soapysdr::Range {
                minimum: 6.0,
                maximum: 10.0,
                step: 2.0,
            },
        ])
        .unwrap();
        assert_eq!(
            range.items(),
            &[
                RangeItem::Value(3.0),
                RangeItem::Interval(4.0, 5.0),
                RangeItem::Step(6.0, 10.0, 2.0),
            ]
        );
    }
}
