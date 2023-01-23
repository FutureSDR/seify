#![allow(unused_variables)]
use num_complex::Complex32;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RangeItem;

#[derive(Clone)]
pub struct Soapy {
    dev: soapysdr::Device,
    index: usize,
}

pub struct RxStreamer {
    streamer: soapysdr::RxStream<Complex32>,
}

pub struct TxStreamer {
    streamer: soapysdr::TxStream<Complex32>,
}

impl Soapy {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        todo!()
    }
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        todo!()
    }
}

impl DeviceTrait for Soapy {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxStreamer;

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> Driver {
        Driver::Soapy
    }

    fn id(&self) -> Result<String, Error> {
        Ok(format!("{}", self.index))
    }

    fn info(&self) -> Result<Args, Error> {
        format!("driver=soapy, index={}", self.index).try_into()
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        Ok(self.dev.num_channels(direction.into())?)
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self.dev.full_duplex(direction.into(), channel)?)
    }

    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, Error> {
        Ok(RxStreamer {
            streamer: self.dev.rx_stream(channels)?,
        })
    }

    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        todo!()
    }

    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, Error> {
        Ok(TxStreamer {
            streamer: self.dev.tx_stream(channels)?,
        })
    }

    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        todo!()
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        Ok(self.dev.antennas(direction.into(), channel)?)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        Ok(self.dev.antenna(direction.into(), channel)?)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        Ok(self.dev.set_antenna(direction.into(), channel, name)?)
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        Ok(self.dev.list_gains(direction.into(), channel)?)
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self.dev.has_gain_mode(direction.into(), channel)?)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        Ok(self.dev.set_gain_mode(direction.into(), channel, agc)?)
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self.dev.gain_mode(direction.into(), channel)?)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        Ok(self.dev.set_gain(direction.into(), channel, gain)?)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        if self.agc(direction, channel)? {
            Ok(None)
        } else {
            Ok(Some(self.dev.gain(direction.into(), channel)?))
        }
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let range = self.dev.gain_range(direction.into(), channel)?;
        Ok(range.into())
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
        if self.agc(direction, channel)? {
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
        Ok(range.into())
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        let mut range = self.dev.frequency_range(direction.into(), channel)?;
        // todo
        Ok(range.remove(0).into())
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        Ok(self.dev.frequency(direction.into(), channel)?)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
    ) -> Result<(), Error> {
        Ok(self
            .dev
            .set_frequency(direction.into(), channel, frequency, "")?)
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
        let mut range = self
            .dev
            .component_frequency_range(direction.into(), channel, name)?;
        // todo
        Ok(range.remove(0).into())
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
        args: Args,
    ) -> Result<(), Error> {
        // todo
        Ok(self
            .dev
            .set_component_frequency(direction.into(), channel, name, frequency, "")?)
    }

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
        let mut range = self.dev.get_sample_rate_range(direction.into(), channel)?;
        // todo 
        Ok(range.remove(0).into())
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(self.streamer.mtu()?)
    }

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        Ok(self.streamer.activate(time_ns)?)
    }

    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
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

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        Ok(self.streamer.activate(time_ns)?)
    }

    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
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
        Error::DeviceError
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

impl From<soapysdr::Range> for Range {
    fn from(range: soapysdr::Range) -> Self {
        let mut r = vec![];
        let mut v = range.minimum;
        loop {
            r.push(RangeItem::Value(v));
            v += range.step;

            if v > range.maximum {
                break;
            }
        }
        Range::new(r)
    }
}

impl TryFrom<Args> for soapysdr::Args {
    type Error = Error;

    fn try_from(value: Args) -> Result<Self, Self::Error> {
        let mut s = String::from("");
        args
    }
}

