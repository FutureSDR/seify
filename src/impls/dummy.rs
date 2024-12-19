//! Dummy SDR for CI
use std::sync::Arc;
use std::sync::Mutex;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Direction::Rx;
use crate::Direction::Tx;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RangeItem;

/// Dummy Device
#[derive(Clone)]
pub struct Dummy {
    rx_agc: Arc<Mutex<bool>>,
    rx_bw: Arc<Mutex<f64>>,
    rx_freq: Arc<Mutex<f64>>,
    rx_gain: Arc<Mutex<f64>>,
    rx_rate: Arc<Mutex<f64>>,
    tx_agc: Arc<Mutex<bool>>,
    tx_bw: Arc<Mutex<f64>>,
    tx_freq: Arc<Mutex<f64>>,
    tx_gain: Arc<Mutex<f64>>,
    tx_rate: Arc<Mutex<f64>>,
}

/// Dummy RX Streamer
pub struct RxStreamer;

/// Dummy TX Streamer
pub struct TxStreamer;

impl Dummy {
    /// Get a list of Devices
    ///
    /// Will only return exactly one device, if `dummy` is set as driver.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        match args.get::<String>("driver").as_deref() {
            Ok("dummy") => {
                let mut a = Args::new();
                a.set("driver", "dummy");
                Ok(vec![a])
            }
            _ => Ok(Vec::new()),
        }
    }
    /// Create a Dummy Device
    pub fn open<A: TryInto<Args>>(_args: A) -> Result<Self, Error> {
        Ok(Self {
            rx_agc: Arc::new(Mutex::new(false)),
            rx_gain: Arc::new(Mutex::new(0.0)),
            rx_freq: Arc::new(Mutex::new(0.0)),
            rx_rate: Arc::new(Mutex::new(0.0)),
            rx_bw: Arc::new(Mutex::new(0.0)),
            tx_agc: Arc::new(Mutex::new(false)),
            tx_gain: Arc::new(Mutex::new(0.0)),
            tx_freq: Arc::new(Mutex::new(0.0)),
            tx_rate: Arc::new(Mutex::new(0.0)),
            tx_bw: Arc::new(Mutex::new(0.0)),
        })
    }
}

impl DeviceTrait for Dummy {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxStreamer;

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> Driver {
        Driver::Dummy
    }

    fn id(&self) -> Result<String, Error> {
        Ok("dummy".to_string())
    }

    fn info(&self) -> Result<Args, Error> {
        let mut a = Args::new();
        a.set("driver", "dummy");
        Ok(a)
    }

    fn num_channels(&self, _direction: Direction) -> Result<usize, Error> {
        Ok(1)
    }

    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(true)
    }

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        match channels {
            &[0] => Ok(RxStreamer),
            _ => Err(Error::ValueError),
        }
    }

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        match channels {
            &[0] => Ok(TxStreamer),
            _ => Err(Error::ValueError),
        }
    }

    fn antennas(&self, _direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        if channel == 0 {
            Ok(vec!["A".to_string()])
        } else {
            Err(Error::ValueError)
        }
    }

    fn antenna(&self, _direction: Direction, channel: usize) -> Result<String, Error> {
        if channel == 0 {
            Ok("A".to_string())
        } else {
            Err(Error::ValueError)
        }
    }

    fn set_antenna(&self, _direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        match (channel, name) {
            (0, "A") => Ok(()),
            _ => Err(Error::ValueError),
        }
    }

    fn gain_elements(&self, _direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        if channel == 0 {
            Ok(vec!["RF".to_string()])
        } else {
            Err(Error::ValueError)
        }
    }

    fn supports_agc(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
        if channel == 0 {
            Ok(true)
        } else {
            Err(Error::ValueError)
        }
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        match (channel, direction) {
            (0, Rx) => {
                *self.rx_agc.lock().unwrap() = agc;
                Ok(())
            }
            (0, Tx) => {
                *self.tx_agc.lock().unwrap() = agc;
                Ok(())
            }
            _ => Err(Error::ValueError),
        }
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        match (channel, direction) {
            (0, Rx) => Ok(*self.rx_agc.lock().unwrap()),
            (0, Tx) => Ok(*self.tx_agc.lock().unwrap()),
            _ => Err(Error::ValueError),
        }
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        if channel == 0 && gain >= 0.0 {
            match direction {
                Rx => *self.rx_gain.lock().unwrap() = gain,
                Tx => *self.tx_gain.lock().unwrap() = gain,
            }
            Ok(())
        } else {
            Err(Error::ValueError)
        }
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        match (channel, direction) {
            (0, Rx) => {
                if *self.rx_agc.lock().unwrap() {
                    Ok(None)
                } else {
                    Ok(Some(*self.rx_gain.lock().unwrap()))
                }
            }
            (0, Tx) => {
                if *self.tx_agc.lock().unwrap() {
                    Ok(None)
                } else {
                    Ok(Some(*self.tx_gain.lock().unwrap()))
                }
            }
            _ => Err(Error::ValueError),
        }
    }

    fn gain_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::ValueError)
        }
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        if channel == 0 && name == "RF" && gain >= 0.0 {
            match direction {
                Rx => *self.rx_gain.lock().unwrap() = gain,
                Tx => *self.tx_gain.lock().unwrap() = gain,
            }
            Ok(())
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
        match (channel, direction, name) {
            (0, Direction::Rx, "RF") => {
                if *self.rx_agc.lock().unwrap() {
                    Ok(None)
                } else {
                    Ok(Some(*self.rx_gain.lock().unwrap()))
                }
            }
            (0, Direction::Tx, "RF") => {
                if *self.tx_agc.lock().unwrap() {
                    Ok(None)
                } else {
                    Ok(Some(*self.tx_gain.lock().unwrap()))
                }
            }
            _ => Err(Error::ValueError),
        }
    }

    fn gain_element_range(
        &self,
        _direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        if channel == 0 && name == "RF" {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::ValueError)
        }
    }

    fn frequency_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::ValueError)
        }
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (channel, direction) {
            (0, Rx) => Ok(*self.rx_freq.lock().unwrap()),
            (0, Tx) => Ok(*self.tx_freq.lock().unwrap()),
            _ => Err(Error::ValueError),
        }
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        if channel == 0 && frequency >= 0.0 {
            match direction {
                Rx => *self.rx_freq.lock().unwrap() = frequency,
                Tx => *self.tx_freq.lock().unwrap() = frequency,
            }
            Ok(())
        } else {
            Err(Error::ValueError)
        }
    }

    fn frequency_components(
        &self,
        _direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        if channel == 0 {
            Ok(vec!["freq".to_string()])
        } else {
            Err(Error::ValueError)
        }
    }

    fn component_frequency_range(
        &self,
        _direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        if channel == 0 && name == "freq" {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::ValueError)
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        if channel == 0 && name == "freq" {
            match direction {
                Rx => Ok(*self.rx_freq.lock().unwrap()),
                Tx => Ok(*self.tx_freq.lock().unwrap()),
            }
        } else {
            Err(Error::ValueError)
        }
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        if channel == 0 && name == "freq" && frequency >= 0.0 {
            match direction {
                Rx => {
                    *self.rx_freq.lock().unwrap() = frequency;
                    Ok(())
                }
                Tx => {
                    *self.tx_freq.lock().unwrap() = frequency;
                    Ok(())
                }
            }
        } else {
            Err(Error::ValueError)
        }
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (channel, direction) {
            (0, Rx) => Ok(*self.rx_rate.lock().unwrap()),
            (0, Tx) => Ok(*self.tx_rate.lock().unwrap()),
            _ => Err(Error::ValueError),
        }
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        if channel == 0 && rate >= 0.0 {
            match direction {
                Rx => *self.rx_rate.lock().unwrap() = rate,
                Tx => *self.tx_rate.lock().unwrap() = rate,
            }
            Ok(())
        } else {
            Err(Error::ValueError)
        }
    }

    fn get_sample_rate_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::ValueError)
        }
    }

    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (channel, direction) {
            (0, Rx) => Ok(*self.rx_bw.lock().unwrap()),
            (0, Tx) => Ok(*self.tx_bw.lock().unwrap()),
            _ => Err(Error::ValueError),
        }
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        if channel == 0 && bw >= 0.0 {
            match direction {
                Rx => *self.rx_bw.lock().unwrap() = bw,
                Tx => *self.tx_bw.lock().unwrap() = bw,
            }
            Ok(())
        } else {
            Err(Error::ValueError)
        }
    }

    fn get_bandwidth_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::ValueError)
        }
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
        Ok(1500)
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        for b in buffers.iter_mut() {
            b.fill(num_complex::Complex32::new(0.0, 0.0))
        }
        Ok(buffers[0].len())
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(1500)
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        Ok(buffers[0].len())
    }

    fn write_all(
        &mut self,
        _buffers: &[&[num_complex::Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<(), Error> {
        Ok(())
    }
}
