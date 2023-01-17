#![allow(dead_code)]
#![allow(unused_variables)]
use rtlsdr_rs::RtlSdr as Sdr;

use crate::Args;
use crate::DeviceTrait;
use crate::Driver;
use crate::Error;

pub struct RtlSdr {
    dev: Sdr,
}
pub struct RxStreamer;
pub struct TxDummy;

impl RtlSdr {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let rtls = rtlsdr_rs::enumerate().or(Err(Error::DeviceError))?;
        let mut devs = Vec::new();
        for r in rtls {
            devs.push(format!("driver=rtlsdr, index={}", r.index).try_into()?);
        }
        Ok(devs)
    }
    pub fn open<A: TryInto<Args>>(_args: A) -> Result<Self, Error> {
        Ok(RtlSdr {
            dev: Sdr::open(0).or(Err(Error::DeviceError))?
        })
    }
}

impl DeviceTrait for RtlSdr {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxDummy;

    fn driver(&self) -> crate::Driver {
        Driver::RtlSdr        
    }

    fn id(&self) -> Result<String, Error> {
        todo!()
    }

    fn info(&self) -> Result<Args, Error> {
        todo!()
    }

    fn num_channels(&self, direction: crate::Direction) -> Result<usize, Error> {
        todo!()
    }

    fn full_duplex(&self, direction: crate::Direction, channel: usize) -> Result<bool, Error> {
        todo!()
    }

    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, Error> {
        todo!()
    }

    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        todo!()
    }

    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, Error> {
        todo!()
    }

    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        todo!()
    }

    fn antennas(&self, direction: crate::Direction, channel: usize) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn antenna(&self, direction: crate::Direction, channel: usize) -> Result<String, Error> {
        todo!()
    }

    fn set_antenna(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        todo!()
    }

    fn gain_elements(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn suports_agc(&self, direction: crate::Direction, channel: usize) -> Result<bool, Error> {
        todo!()
    }

    fn enable_agc(
        &self,
        direction: crate::Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        todo!()
    }

    fn agc(&self, direction: crate::Direction, channel: usize) -> Result<bool, Error> {
        todo!()
    }

    fn set_gain(
        &self,
        direction: crate::Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn gain(&self, direction: crate::Direction, channel: usize) -> Result<f64, Error> {
        todo!()
    }

    fn gain_range(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<crate::Range, Error> {
        todo!()
    }

    fn set_gain_element(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn gain_element(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        todo!()
    }

    fn gain_element_range(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<crate::Range, Error> {
        todo!()
    }

    fn frequency_range(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<crate::Range, Error> {
        todo!()
    }

    fn frequency(&self, direction: crate::Direction, channel: usize) -> Result<f64, Error> {
        todo!()
    }

    fn set_frequency(
        &self,
        direction: crate::Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        todo!()
    }

    fn list_frequencies(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn component_frequency_range(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<crate::Range, Error> {
        todo!()
    }

    fn component_frequency(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        todo!()
    }

    fn set_component_frequency(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        todo!()
    }

    fn sample_rate(&self, direction: crate::Direction, channel: usize) -> Result<f64, Error> {
        todo!()
    }

    fn set_sample_rate(
        &self,
        direction: crate::Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn get_sample_rate_range(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<crate::Range, Error> {
        todo!()
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        todo!()
    }
    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        todo!()
    }
    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        todo!()
    }
    fn read(
        &mut self,
        buffers: &[&mut [num_complex::Complex32]],
        timeout_us: i64,
    ) -> Result<usize, Error> {
        todo!()
    }
}

impl crate::TxStreamer for TxDummy {
    fn mtu(&self) -> Result<usize, Error> {
        unreachable!()
    }
    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }
    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }
    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        unreachable!()
    }
    fn write_all(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        unreachable!()
    }
}
