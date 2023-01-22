use num_complex::Complex32;
use rtlsdr_rs::enumerate;
use rtlsdr_rs::RtlSdr as Sdr;
use rtlsdr_rs::TunerGain;
use std::any::Any;
use std::sync::Arc;
use std::sync::Mutex;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Direction::*;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RangeItem;

#[derive(Clone)]
pub struct RtlSdr {
    dev: Arc<Sdr>,
    index: usize,
    i: Arc<Mutex<Inner>>,
}
unsafe impl Send for RtlSdr {}

struct Inner {
    gain: TunerGain,
}

pub struct RxStreamer {
    dev: Arc<Sdr>,
}

unsafe impl Send for RxStreamer {}

impl RxStreamer {
    fn new(dev: Arc<Sdr>) -> Self {
        Self { dev }
    }
}

pub struct TxDummy;
unsafe impl Send for TxDummy {}

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
        let dev = RtlSdr {
            dev: Arc::new(Sdr::open(index).or(Err(Error::DeviceError))?),
            index,
            i: Arc::new(Mutex::new(Inner {
                gain: TunerGain::Auto,
            })),
        };
        dev.enable_agc(Rx, 0, true)?;
        Ok(dev)
    }
}

impl DeviceTrait for RtlSdr {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxDummy;

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

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
            Rx => Ok(1),
            Tx => Ok(0),
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
        _args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            Err(Error::ValueError)
        } else {
            Ok(RxStreamer::new(self.dev.clone()))
        }
    }

    fn tx_stream(&self, _channels: &[usize]) -> Result<Self::TxStreamer, Error> {
        Err(Error::NotSupported)
    }

    fn tx_stream_with_args(
        &self,
        _channels: &[usize],
        _args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        Err(Error::NotSupported)
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.antenna(direction, channel).map(|a| vec![a])
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok("RX".to_string())
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        if matches!(direction, Rx) && channel == 0 && name == "RX" {
            Ok(())
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(vec!["TUNER".to_string()])
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(true)
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        let gains = self.dev.get_tuner_gains().or(Err(Error::DeviceError))?;
        if matches!(direction, Rx) && channel == 0 {
            let mut inner = self.i.lock().unwrap();
            if agc {
                inner.gain = TunerGain::Auto;
                self.dev
                    .set_tuner_gain(inner.gain.clone())
                    .or(Err(Error::DeviceError))
            } else {
                inner.gain = TunerGain::Manual(gains[gains.len() / 2]);
                self.dev
                    .set_tuner_gain(inner.gain.clone())
                    .or(Err(Error::DeviceError))
            }
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        if matches!(direction, Rx) && channel == 0 {
            let inner = self.i.lock().unwrap();
            Ok(matches!(inner.gain, TunerGain::Auto))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.set_gain_element(direction, channel, "TUNER", gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.gain_element(direction, channel, "TUNER")
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.gain_element_range(direction, channel, "TUNER")
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let r = self.gain_range(direction, channel)?;
        if r.contains(gain) && name == "TUNER" {
            let mut inner = self.i.lock().unwrap();
            inner.gain = TunerGain::Manual(gain as i32);
            self.dev
                .set_tuner_gain(inner.gain.clone())
                .or(Err(Error::ValueError))
        } else {
            Err(Error::ValueError)
        }
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        if matches!(direction, Rx) && channel == 0 && name == "TUNER" {
            let inner = self.i.lock().unwrap();
            match inner.gain {
                TunerGain::Auto => Ok(None),
                TunerGain::Manual(i) => Ok(Some(i as f64)),
            }
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        if matches!(direction, Rx) && channel == 0 && name == "TUNER" {
            let gains = self.dev.get_tuner_gains().or(Err(Error::DeviceError))?;
            Ok(Range::new(
                gains.iter().map(|g| RangeItem::Value(*g as f64)).collect(),
            ))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.component_frequency_range(direction, channel, "TUNER")
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.component_frequency(direction, channel, "TUNER")
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        self.set_component_frequency(direction, channel, "TUNER", frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(vec!["TUNER".to_string()])
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        if matches!(direction, Rx) && channel == 0 && name == "TUNER" {
            Ok(Range::new(vec![RangeItem::Interval(0.0, 2e9)]))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        if matches!(direction, Rx) && channel == 0 && name == "TUNER" {
            Ok(self.dev.get_center_freq() as f64)
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        if matches!(direction, Rx)
            && channel == 0
            && self
                .frequency_range(direction, channel)?
                .contains(frequency)
            && name == "TUNER"
        {
            self.dev
                .set_center_freq(frequency as u32)
                .or(Err(Error::DeviceError))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(self.dev.get_sample_rate() as f64)
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        if matches!(direction, Rx)
            && channel == 0
            && self
                .get_sample_rate_range(direction, channel)?
                .contains(rate)
        {
            self.dev
                .set_sample_rate(rate as u32)
                .or(Err(Error::DeviceError))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(Range::new(vec![
                RangeItem::Interval(225_001.0, 300_000.0),
                RangeItem::Interval(900_001.0, 3_200_000.0),
            ]))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(16 * 16384)
    }
    fn activate(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        self.dev.reset_buffer().or(Err(Error::DeviceError))
    }
    fn deactivate(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }
    fn read(&mut self, buffers: &mut [&mut [Complex32]], _timeout_us: i64) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);
        let mut u = vec![0u8; buffers[0].len() * 2];
        let n = self.dev.read_sync(&mut u).or(Err(Error::DeviceError))?;
        debug_assert_eq!(n % 2, 0);

        for i in 0..n / 2 {
            buffers[0][i] = Complex32::new(u[i * 2] as f32 - 127.0, u[i * 2 + 1] as f32 - 127.0);
        }
        Ok(n / 2)
    }
}

impl crate::TxStreamer for TxDummy {
    fn mtu(&self) -> Result<usize, Error> {
        unreachable!()
    }
    fn activate(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }
    fn deactivate(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }
    fn write(
        &mut self,
        _buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        unreachable!()
    }
    fn write_all(
        &mut self,
        _buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<(), Error> {
        unreachable!()
    }
}
