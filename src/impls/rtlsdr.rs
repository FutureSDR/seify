//! RTL SDR
use num_complex::Complex32;
use seify_rtlsdr::enumerate;
use seify_rtlsdr::RtlSdr as Sdr;
use seify_rtlsdr::TunerGain;
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

const MTU: usize = 4 * 16384;

/// Rusty RTL-SDR driver
#[derive(Clone)]
pub struct RtlSdr {
    dev: Arc<Sdr>,
    index: usize,
    i: Arc<Mutex<Inner>>,
}
unsafe impl Send for RtlSdr {}
unsafe impl Sync for RtlSdr {}

struct Inner {
    gain: TunerGain,
}

/// Rusty RTL-SDR RX streamer
pub struct RxStreamer {
    dev: Arc<Sdr>,
    buf: [u8; MTU],
}

unsafe impl Send for RxStreamer {}

impl RxStreamer {
    fn new(dev: Arc<Sdr>) -> Self {
        Self { dev, buf: [0; MTU] }
    }
}

/// Rusty RTL-SDR TX dummy streamer
pub struct TxDummy;
unsafe impl Send for TxDummy {}

impl RtlSdr {
    /// Get a list of detected RTL-SDR devices
    ///
    /// The returned [`Args`] specify the device, i.e., passing them to [`RtlSdr::open`] will open
    /// this particular device. At the moment, this just uses the index in the list of devices
    /// returned by the driver.
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let rtls = enumerate().or(Err(Error::DeviceError))?;
        let mut devs = Vec::new();
        for r in rtls {
            devs.push(format!("driver=rtlsdr, index={}, serial={}", r.index, r.serial).try_into()?);
        }
        Ok(devs)
    }
    /// Create an RTL-SDR device
    ///
    /// At the moment, only an `index` argument is considered, which defines the index of the
    /// devices in the list returned by the driver.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args.try_into().or(Err(Error::ValueError))?;
        let rtls = enumerate().or(Err(Error::DeviceError))?;

        let index = match args
            .get::<usize>("index")
            .map_err(|_| args.get::<String>("serial"))
        {
            Ok(index) => index,
            Err(Ok(serial)) => rtls
                .iter()
                .position(|rtl| rtl.serial == serial)
                .ok_or(Error::NotFound)?,
            Err(Err(_)) => 0,
        };
        if index >= rtls.len() {
            return Err(Error::NotFound);
        }
        #[allow(clippy::arc_with_non_send_sync)]
        let dev = Arc::new(Sdr::open(index)?);
        dev.set_tuner_gain(TunerGain::Auto)?;
        dev.set_bias_tee(false)?;
        let dev = RtlSdr {
            dev,
            index,
            i: Arc::new(Mutex::new(Inner {
                gain: TunerGain::Auto,
            })),
        };
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

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            Err(Error::ValueError)
        } else {
            Ok(RxStreamer::new(self.dev.clone()))
        }
    }

    fn tx_streamer(&self, _channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
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

    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
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
                Ok(self.dev.set_tuner_gain(inner.gain.clone())?)
            } else {
                inner.gain = TunerGain::Manual(gains[gains.len() / 2]);
                Ok(self.dev.set_tuner_gain(inner.gain.clone())?)
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
            inner.gain = TunerGain::Manual((gain * 10.0) as i32);
            Ok(self.dev.set_tuner_gain(inner.gain.clone())?)
        } else {
            log::warn!("Gain out of range");
            Err(Error::OutOfRange(r, gain))
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
            Ok(Range::new(vec![RangeItem::Interval(0.0, 50.0)]))
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
        _args: Args,
    ) -> Result<(), Error> {
        self.set_component_frequency(direction, channel, "TUNER", frequency)
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
    ) -> Result<(), Error> {
        if matches!(direction, Rx)
            && channel == 0
            && self
                .frequency_range(direction, channel)?
                .contains(frequency)
            && name == "TUNER"
        {
            self.dev.set_center_freq(frequency as u32)?;
            Ok(self.dev.reset_buffer()?)
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
            self.dev.set_tuner_bandwidth(rate as u32)?;
            Ok(self.dev.set_sample_rate(rate as u32)?)
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

    fn bandwidth(&self, _direction: Direction, _channel: usize) -> Result<f64, Error> {
        Err(Error::NotSupported)
    }

    fn set_bandwidth(&self, _direction: Direction, _channel: usize, bw: f64) -> Result<(), Error> {
        Ok(self.dev.set_tuner_bandwidth(bw as _)?)
    }

    fn get_bandwidth_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        Err(Error::NotSupported)
    }

    fn has_dc_offset_mode(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Err(Error::NotSupported)
    }

    fn set_dc_offset_mode(
        &self,
        _direction: Direction,
        _channel: usize,
        _automatic: bool,
    ) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    fn dc_offset_mode(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Err(Error::NotSupported)
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(MTU)
    }
    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        self.dev.reset_buffer().or(Err(Error::DeviceError))
    }
    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }
    fn read(&mut self, buffers: &mut [&mut [Complex32]], _timeout_us: i64) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);
        // make len multiple of 256 to make u multiple of 512
        let len = std::cmp::min(buffers[0].len(), MTU / 2);
        let len = len & !0xff;
        if len == 0 {
            return Ok(0);
        }
        let n = self.dev.read_sync(&mut self.buf[0..len * 2])?;
        debug_assert_eq!(n % 2, 0);

        #[allow(clippy::needless_range_loop)]
        for i in 0..n / 2 {
            buffers[0][i] = Complex32::new(
                (self.buf[i * 2] as f32 - 127.0) / 128.0,
                (self.buf[i * 2 + 1] as f32 - 127.0) / 128.0,
            );
        }
        Ok(n / 2)
    }
}

impl crate::TxStreamer for TxDummy {
    fn mtu(&self) -> Result<usize, Error> {
        unreachable!()
    }
    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }
    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
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
