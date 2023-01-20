#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(dead_code)]
use std::sync::Arc;

use aaronia_rtsa::ApiHandle;
use aaronia_rtsa::Device as Sdr;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Direction::*;
use crate::Driver;
use crate::Error;

#[derive(Debug)]
pub struct Aaronia {
    dev: Arc<Sdr>,
    index: usize,
}
pub struct RxStreamer {
    dev: Arc<Sdr>,
}
impl RxStreamer {
    fn new(dev: Arc<Sdr>) -> Self {
        Self { dev }
    }
}

pub struct TxStreamer {
    dev: Arc<Sdr>,
}
impl TxStreamer {
    fn new(dev: Arc<Sdr>) -> Self {
        Self { dev }
    }
}

impl Aaronia {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let mut api = ApiHandle::new().or(Err(Error::DeviceError))?;
        api.rescan_devices().or(Err(Error::DeviceError))?;
        let devs = api.devices().or(Err(Error::DeviceError))?;
        Ok(devs
            .iter()
            .enumerate()
            .map(|(i, d)| format!("index={i}, driver=aaronia").parse().unwrap())
            .collect())
    }

    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let mut api = ApiHandle::new().or(Err(Error::DeviceError))?;
        api.rescan_devices().or(Err(Error::DeviceError))?;
        let devs = api.devices().or(Err(Error::DeviceError))?;

        let args = args.try_into().or(Err(Error::ValueError))?;
        let index = args.get::<usize>("index").unwrap_or(0);

        let dev = api
            .get_this_device(&devs[index])
            .or(Err(Error::DeviceError))?;
        Ok(Aaronia {
            dev: Arc::new(dev),
            index,
        })
    }
}

impl DeviceTrait for Aaronia {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxStreamer;

    fn driver(&self) -> crate::Driver {
        Driver::Aaronia
    }

    fn id(&self) -> Result<String, crate::Error> {
        Ok(format!("{}", self.index))
    }

    fn info(&self) -> Result<crate::Args, crate::Error> {
        format!("driver=aaronia, index={}", self.index).try_into()
    }

    fn num_channels(&self, _direction: Direction) -> Result<usize, crate::Error> {
        Ok(1)
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, crate::Error> {
        if channel == 0 {
            Ok(true)
        } else {
            Err(Error::ValueError)
        }
    }

    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, crate::Error> {
        self.rx_stream_with_args(channels, Args::new())
    }

    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        args: crate::Args,
    ) -> Result<Self::RxStreamer, crate::Error> {
        if channels == &[0] {
            Ok(RxStreamer::new(self.dev.clone()))
        } else {
            Err(Error::ValueError)
        }
    }

    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, crate::Error> {
        self.tx_stream_with_args(channels, Args::new())
    }

    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: crate::Args,
    ) -> Result<Self::TxStreamer, crate::Error> {
        if channels == &[0] {
            Ok(TxStreamer::new(self.dev.clone()))
        } else {
            Err(Error::ValueError)
        }
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, crate::Error> {
        match direction {
            Rx => match channel {
                0 => Ok(vec!["RX1".to_string()]),
                1 => Ok(vec!["RX2".to_string()]),
                _ => Err(Error::ValueError),
            },
            Tx => match channel {
                0 => Ok(vec!["TX1".to_string()]),
                _ => Err(Error::ValueError),
            },
        }
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, crate::Error> {
        match direction {
            Rx => match channel {
                0 => Ok("RX1".to_string()),
                1 => Ok("RX2".to_string()),
                _ => Err(Error::ValueError),
            },
            Tx => match channel {
                0 => Ok("TX1".to_string()),
                _ => Err(Error::ValueError),
            },
        }
    }

    fn set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), crate::Error> {
        match direction {
            Rx => match (channel, name) {
                (0, "RX1") => Ok(()),
                (1, "RX2") => Ok(()),
                _ => Err(Error::ValueError),
            },
            Tx => match (channel, name) {
                (0, "TX1") => Ok(()),
                _ => Err(Error::ValueError),
            },
        }
    }

    fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, crate::Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(vec!["TUNER".to_string()]),
            (Tx, 0) => Ok(vec!["TUNER".to_string()]),
            _ => Err(Error::ValueError)
        }
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, crate::Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(true),
            (Tx, 0) => Ok(true),
            _ => Err(Error::ValueError)
        }
    }

    fn enable_agc(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, crate::Error> {
        todo!()
    }

    fn set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, crate::Error> {
        todo!()
    }

    fn gain_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, crate::Error> {
        todo!()
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, crate::Error> {
        todo!()
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: crate::Args,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn list_frequencies(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, crate::Error> {
        todo!()
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, crate::Error> {
        todo!()
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
        args: crate::Args,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, crate::Error> {
        todo!()
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, crate::Error> {
        todo!()
    }

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), crate::Error> {
        todo!()
    }

    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), crate::Error> {
        todo!()
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        timeout_us: i64,
    ) -> Result<usize, crate::Error> {
        todo!()
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, crate::Error> {
        todo!()
    }

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), crate::Error> {
        todo!()
    }

    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), crate::Error> {
        todo!()
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, crate::Error> {
        todo!()
    }

    fn write_all(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), crate::Error> {
        todo!()
    }
}
