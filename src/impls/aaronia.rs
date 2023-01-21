#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(dead_code)]
use std::any::Any;
use std::sync::Arc;
use std::sync::Mutex;

use aaronia_rtsa::ApiHandle;
use aaronia_rtsa::ConfigItem;
use aaronia_rtsa::Device as Sdr;
use aaronia_rtsa::Packet;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Direction::*;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RangeItem;

#[derive(Debug)]
pub struct Aaronia {
    dev: Arc<Mutex<Sdr>>,
    index: usize,
}
pub struct RxStreamer {
    dev: Arc<Mutex<Sdr>>,
    packet: Option<(Packet, usize)>,
}
impl RxStreamer {
    fn new(dev: Arc<Mutex<Sdr>>) -> Self {
        Self { dev, packet: None }
    }
}

pub struct TxStreamer {
    dev: Arc<Mutex<Sdr>>,
}
impl TxStreamer {
    fn new(dev: Arc<Mutex<Sdr>>) -> Self {
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

        let mut dev = api
            .get_this_device(&devs[index])
            .or(Err(Error::DeviceError))?;
        dev.open().or(Err(Error::DeviceError))?;
        Ok(Aaronia {
            dev: Arc::new(Mutex::new(dev)),
            index,
        })
    }
}

impl DeviceTrait for Aaronia {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxStreamer;

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn driver(&self) -> crate::Driver {
        Driver::Aaronia
    }

    fn id(&self) -> Result<String, Error> {
        Ok(format!("{}", self.index))
    }

    fn info(&self) -> Result<crate::Args, Error> {
        format!("driver=aaronia, index={}", self.index).try_into()
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        match direction {
            Rx => Ok(2),
            Tx => Ok(1),
        }
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(true),
            (Tx, 0) => Ok(true),
            _ => Err(Error::ValueError),
        }
    }

    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, Error> {
        self.rx_stream_with_args(channels, Args::new())
    }

    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        args: crate::Args,
    ) -> Result<Self::RxStreamer, Error> {
        if channels == [0] {
            Ok(RxStreamer::new(self.dev.clone()))
        } else {
            Err(Error::ValueError)
        }
    }

    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, Error> {
        self.tx_stream_with_args(channels, Args::new())
    }

    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: crate::Args,
    ) -> Result<Self::TxStreamer, Error> {
        if channels == [0] {
            Ok(TxStreamer::new(self.dev.clone()))
        } else {
            Err(Error::ValueError)
        }
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        match (direction, channel) {
            (Rx, 0) => Ok(vec!["RX1".to_string()]),
            (Rx, 1) => Ok(vec!["RX2".to_string()]),
            (Tx, 0) => Ok(vec!["TX1".to_string()]),
            _ => Err(Error::ValueError),
        }
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        match (direction, channel) {
            (Rx, 0) => Ok("RX1".to_string()),
            (Rx, 1) => Ok("RX2".to_string()),
            (Tx, 0) => Ok("TX1".to_string()),
            _ => Err(Error::ValueError),
        }
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        match (direction, channel, name) {
            (Rx, 0, "RX1") => Ok(()),
            (Rx, 1, "RX2") => Ok(()),
            (Tx, 0, "TX1") => Ok(()),
            _ => Err(Error::ValueError),
        }
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(vec!["TUNER".to_string()]),
            (Tx, 0) => Ok(vec!["TUNER".to_string()]),
            _ => Err(Error::ValueError),
        }
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(true),
            (Tx, 0) => Ok(true),
            _ => Err(Error::ValueError),
        }
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        let mut dev = self.dev.lock().unwrap();
        match (direction, channel) {
            (Rx, 0 | 1) => {
                if agc {
                    dev.set("device/gaincontrol", "power")
                        .or(Err(Error::DeviceError))
                } else {
                    dev.set("device/gaincontrol", "manual")
                        .or(Err(Error::DeviceError))
                }
            }
            _ => Err(Error::ValueError),
        }
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        let mut dev = self.dev.lock().unwrap();
        match (direction, channel) {
            (Rx, 0 | 1) => match dev.get("device/gaincontrol").or(Err(Error::DeviceError))? {
                ConfigItem::Enum(0, _) => Ok(false),
                _ => Ok(true),
            },
            _ => Err(Error::ValueError),
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
        let mut dev = self.dev.lock().unwrap();
        match (direction, channel, name) {
            (Rx, 0 | 1, "TUNER") | (Tx, 0, "TUNER") => {
                if (0.0..=30.0).contains(&gain) {
                    dev.set("main/reflevel", format!("{}", -8.0 - gain))
                        .or(Err(Error::DeviceError))
                } else {
                    Err(Error::ValueError)
                }
            }
            _ => Err(Error::DeviceError),
        }
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        match (direction, channel) {
            (Rx, 0) => Ok(None),
            (Rx, 1) => Ok(None),
            (Tx, 0) => todo!(),
            _ => Err(Error::ValueError),
        }
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        match (direction, channel, name) {
            (Rx, 0 | 1, "TUNER") => Ok(Range::new(vec![RangeItem::Interval(0.0, 30.0)])),
            (Tx, 0, "TUNER") => Ok(Range::new(vec![RangeItem::Interval(-100.0, 10.0)])),
            _ => Err(Error::ValueError),
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
        match (direction, channel) {
            (Rx, 0 | 1) | (Tx, 0) => Ok(vec!["TUNER".to_string()]),
            _ => Err(Error::ValueError),
        }
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        match (direction, channel, name) {
            (Rx, 0 | 1, "TUNER") | (Tx, 0, "TUNER") => {
                Ok(Range::new(vec![RangeItem::Interval(193e6, 6e9)]))
            }
            _ => Err(Error::ValueError),
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        match (direction, channel, name) {
            (Rx, 0 | 1, "TUNER") => {
                let mut dev = self.dev.lock().unwrap();
                let s = dev.get("main/centerfreq").or(Err(Error::DeviceError))?;
                match s {
                    ConfigItem::Number(f) => Ok(f),
                    _ => Err(Error::ValueError),
                }
            }
            _ => Err(Error::ValueError),
        }
    }

    fn set_component_frequency(
        &self,
        _direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        let mut dev = self.dev.lock().unwrap();
        match (channel, name) {
            (0 | 1, "TUNER") => dev
                .set("main/centerfreq", format!("{frequency}"))
                .or(Err(Error::DeviceError)),
            _ => Err(Error::ValueError),
        }
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let mut dev = self.dev.lock().unwrap();
                let s = dev
                    .get("device/receiverclock")
                    .or(Err(Error::DeviceError))?;
                let rate = match s {
                    ConfigItem::Enum(0, _) => 92e6,
                    ConfigItem::Enum(1, _) => 122e6,
                    ConfigItem::Enum(2, _) => 184e6,
                    ConfigItem::Enum(3, _) => 245e6,
                    _ => return Err(Error::ValueError),
                };
                let s = dev.get("main/decimation").or(Err(Error::DeviceError))?;
                let s = match s {
                    ConfigItem::Enum(0, _) => 1.0,
                    ConfigItem::Enum(1, _) => 2.0,
                    ConfigItem::Enum(2, _) => 4.0,
                    ConfigItem::Enum(3, _) => 8.0,
                    ConfigItem::Enum(4, _) => 16.0,
                    ConfigItem::Enum(5, _) => 32.0,
                    ConfigItem::Enum(6, _) => 64.0,
                    ConfigItem::Enum(7, _) => 128.0,
                    ConfigItem::Enum(8, _) => 256.0,
                    ConfigItem::Enum(9, _) => 512.0,
                    _ => return Err(Error::ValueError),
                };

                Ok(rate / s)
            }
            _ => Err(Error::ValueError),
        }
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let mut dev = self.dev.lock().unwrap();
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let dec = vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0];
                for (i, d) in dec.into_iter().enumerate() {
                    if (rate - 92e6 / d).abs() < 0.00001 {
                        dev.set("device/receiverclock", "92MHz")
                            .or(Err(Error::DeviceError))?;
                        return dev.set_int("main/decimation", i as i64).or(Err(Error::DeviceError))
                    }
                }
                Err(Error::ValueError)
            }
            (Tx, 0) => todo!(),
            _ => Err(Error::ValueError),
        }
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(Range::new(
                vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0]
                    .into_iter()
                    .map(|v| RangeItem::Value(92e6 / v))
                    .collect(),
            )),
            (Tx, 0) => todo!(),
            _ => Err(Error::ValueError),
        }
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(1024)
    }

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        let mut dev = self.dev.lock().unwrap();
        dev.connect().or(Err(Error::DeviceError))?;
        dev.start().or(Err(Error::DeviceError))
    }

    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        let mut dev = self.dev.lock().unwrap();
        dev.stop().or(Err(Error::DeviceError))?;
        dev.disconnect().or(Err(Error::DeviceError))
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        let mut dev = self.dev.lock().unwrap();
        debug_assert_eq!(buffers.len(), 1);

        let mut i = 0;
        let len = buffers[0].len();
        while i < len {
            match self.packet.take() {
                None => {
                    let p = dev.packet(0).or(Err(Error::DeviceError))?;
                    let cur = p.samples();
                    let n = std::cmp::min(len - i, cur.len());
                    buffers[0][i..i + n].copy_from_slice(&cur[0..n]);
                    i += n;
                    if n == cur.len() {
                        dev.consume(0).or(Err(Error::DeviceError))?;
                    } else {
                        self.packet = Some((p, n));
                    }
                }
                Some((p, offset)) => {
                    let cur = p.samples();
                    let n = std::cmp::min(len - i, cur.len() - offset);
                    buffers[0][i..i + n].copy_from_slice(&cur[offset..offset+n]);
                    i += n;
                    if offset + n == cur.len() {
                        dev.consume(0).or(Err(Error::DeviceError))?;
                    } else {
                        self.packet = Some((p, offset + n));
                    }
                }
            }
        }
        
        Ok(len)
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(1024)
    }

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        todo!()
    }

    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        todo!()
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        todo!()
    }

    fn write_all(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        todo!()
    }
}
