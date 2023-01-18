#![allow(dead_code)]
#![allow(unused_variables)]
use rtlsdr_rs::RtlSdr as Sdr;
use rtlsdr_rs::TunerGain;
use rtlsdr_rs::enumerate;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Driver;
use crate::Error;

pub struct RtlSdr {
    dev: Sdr,
    index: usize,
}
pub struct RxStreamer;

impl RxStreamer {
    fn new(sdr: &Sdr) -> Self {
        todo!()
    }
}

pub struct TxDummy;

impl RtlSdr {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let rtls = enumerate().or(Err(Error::DeviceError))?;
        let mut devs = Vec::new();
        for r in rtls {
            devs.push(format!("driver=rtlsdr, index={}", r.index).try_into()?);
        }
        Ok(devs)
    }
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args.try_into().or(Err(Error::ValueError))?;
        let index = args.get::<usize>("index").unwrap_or(0);
        Ok(RtlSdr {
            dev: Sdr::open(index).or(Err(Error::DeviceError))?,
            index,
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
        Ok(format!("{}", self.index))
    }

    fn info(&self) -> Result<Args, Error> {
        format!("driver=rtlsdr, index={}", self.index).try_into()
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        match direction {
            Direction::Rx => Ok(1),
            Direction::Tx => Ok(0),
        }
    }

    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(false)
    }

    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, Error> {
        self.rx_stream_with_args(channels, Args::new())
    }

    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            Err(Error::ValueError)
        } else {
            Ok(RxStreamer::new(&self.dev))
        }
    }

    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, Error> {
        Err(Error::NotSupported)
    }

    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        Err(Error::NotSupported)
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.antenna(direction, channel).map(|a| vec![a])
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        if matches!(direction, Direction::Rx) && channel == 0 {
            Ok("RX".to_string())
        } else if matches!(direction, Direction::Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_antenna(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), Error> {
        if matches!(direction, Direction::Rx) && channel == 0 && name == "RX" {
            Ok(())
        } else if matches!(direction, Direction::Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        if matches!(direction, Direction::Rx) && channel == 0 {
            Ok(vec!["TUNER".to_string()])
        } else if matches!(direction, Direction::Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        if matches!(direction, Direction::Rx) && channel == 0 {
            Ok(true)
        } else if matches!(direction, Direction::Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn enable_agc(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        let gains = self.dev.get_tuner_gains().or(Err(Error::DeviceError))?;
        if matches!(direction, Direction::Rx) && channel == 0 {
            if agc {
                self.dev.set_tuner_gain(TunerGain::Auto).or(Err(Error::DeviceError))
            } else {
                self.dev.set_tuner_gain(TunerGain::Manual(gains[gains.len() / 2])).or(Err(Error::DeviceError))         
            }
        } else if matches!(direction, Direction::Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        todo!()
    }

    fn set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        todo!()
    }

    fn gain_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<crate::Range, Error> {
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

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        todo!()
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<crate::Range, Error> {
        todo!()
    }

    fn frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<crate::Range, Error> {
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

    fn list_frequencies(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<crate::Range, Error> {
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

    fn get_sample_rate_range(
        &self,
        direction: Direction,
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
