use std::{
    os::fd::{FromRawFd, OwnedFd},
    sync::{Arc, Mutex},
};

use num_complex::Complex32;
use seify_hackrfone::Config;

use crate::{Args, Direction, Error, Range, RangeItem};

pub struct HackRfOne {
    inner: Arc<HackRfInner>,
}

const MTU: usize = 64 * 1024;

impl HackRfOne {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let mut devs = vec![];
        for (bus_number, address) in seify_hackrfone::HackRf::scan()? {
            log::debug!("probing {bus_number}:{address}");
            devs.push(
                format!(
                    "driver=hackrfone, bus_number={}, address={}",
                    bus_number, address
                )
                .try_into()?,
            );
        }
        Ok(devs)
    }

    /// Create a Hackrf One devices
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args: Args = args.try_into().or(Err(Error::ValueError))?;

        if let Ok(fd) = args.get::<i32>("fd") {
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };

            return Ok(Self {
                inner: Arc::new(HackRfInner {
                    dev: seify_hackrfone::HackRf::from_fd(fd)?,
                    tx_config: Mutex::new(Config::tx_default()),
                    rx_config: Mutex::new(Config::rx_default()),
                }),
            });
        }

        let bus_number = args.get("bus_number");
        let address = args.get("address");
        let dev = match (bus_number, address) {
            (Ok(bus_number), Ok(address)) => {
                seify_hackrfone::HackRf::open_bus(bus_number, address)?
            }
            (Err(Error::NotFound), Err(Error::NotFound)) => {
                log::debug!("Opening first hackrf device");
                seify_hackrfone::HackRf::open_first()?
            }
            (bus_number, address) => {
                log::warn!("HackRfOne::open received invalid args: bus_number: {bus_number:?}, address: {address:?}");
                return Err(Error::ValueError);
            }
        };

        Ok(Self {
            inner: Arc::new(HackRfInner {
                dev,
                tx_config: Mutex::new(Config::tx_default()),
                rx_config: Mutex::new(Config::rx_default()),
            }),
        })
    }

    pub fn with_config<F, R>(&self, direction: Direction, f: F) -> R
    where
        F: FnOnce(&mut Config) -> R,
    {
        let config = match direction {
            Direction::Tx => self.inner.tx_config.lock(),
            Direction::Rx => self.inner.rx_config.lock(),
        };
        f(&mut config.unwrap())
    }
}

struct HackRfInner {
    dev: seify_hackrfone::HackRf,
    tx_config: Mutex<seify_hackrfone::Config>,
    rx_config: Mutex<seify_hackrfone::Config>,
}

pub struct RxStreamer {
    inner: Arc<HackRfInner>,
    stream: Option<seify_hackrfone::RxStream>,
}

impl RxStreamer {
    fn new(inner: Arc<HackRfInner>) -> Self {
        Self {
            inner,
            stream: None,
        }
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(MTU)
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        // TODO: sleep precisely for `time_ns`
        let config = self.inner.rx_config.lock().unwrap();
        self.inner.dev.start_rx(&config)?;

        self.stream = Some(self.inner.dev.start_rx_stream(MTU)?);

        Ok(())
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        // TODO: sleep precisely for `time_ns`

        let _ = self.stream.take().unwrap();
        self.inner.dev.stop_rx()?;
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);

        if buffers[0].len() == 0 {
            return Ok(0);
        }
        let buf = self.stream.as_mut().unwrap().read_sync(buffers[0].len())?;

        let samples = buf.len() / 2;
        for i in 0..samples {
            buffers[0][i] = Complex32::new(
                (buf[i * 2] as f32 - 127.0) / 128.0,
                (buf[i * 2 + 1] as f32 - 127.0) / 128.0,
            );
        }
        Ok(samples)
    }
}

pub struct TxStreamer {
    inner: Arc<HackRfInner>,
}

impl TxStreamer {
    fn new(inner: Arc<HackRfInner>) -> Self {
        Self { inner }
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(MTU)
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        // TODO: sleep precisely for `time_ns`

        let config = self.inner.tx_config.lock().unwrap();
        self.inner.dev.start_rx(&config)?;

        Ok(())
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        // TODO: sleep precisely for `time_ns`

        self.inner.dev.stop_tx()?;
        Ok(())
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);
        todo!();

        // self.inner.dev.write(samples)
    }

    fn write_all(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<(), Error> {
        debug_assert_eq!(buffers.len(), 1);

        let mut n = 0;
        while n < buffers[0].len() {
            let buf = &buffers[0][n..];
            n += self.write(&[buf], None, false, 0)?;
        }

        Ok(())
    }
}

impl crate::DeviceTrait for HackRfOne {
    type RxStreamer = RxStreamer;

    type TxStreamer = TxStreamer;

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> crate::Driver {
        crate::Driver::HackRf
    }

    fn id(&self) -> Result<String, Error> {
        Ok(self.inner.dev.board_id()?.to_string())
    }

    fn info(&self) -> Result<crate::Args, Error> {
        let mut args = crate::Args::default();
        args.set("firmware version", self.inner.dev.version()?);
        Ok(args)
    }

    fn num_channels(&self, _: crate::Direction) -> Result<usize, Error> {
        Ok(1)
    }

    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(false)
    }

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            Err(Error::ValueError)
        } else {
            Ok(RxStreamer::new(Arc::clone(&self.inner)))
        }
    }

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        if channels != [0] {
            Err(Error::ValueError)
        } else {
            Ok(TxStreamer::new(Arc::clone(&self.inner)))
        }
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.antenna(direction, channel).map(|a| vec![a])
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        if channel == 0 {
            Ok(match direction {
                Direction::Rx => "RX".to_string(),
                Direction::Tx => "TX".to_string(),
            })
        } else {
            Err(Error::ValueError)
        }
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        if channel == 0 {
            if direction == Direction::Rx && name == "RX"
                || direction == Direction::Tx && name == "TX"
            {
                Ok(())
            } else {
                Err(Error::NotSupported)
            }
        } else {
            Err(Error::ValueError)
        }
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        if channel == 0 {
            // TODO: add support for other gains (RF and baseband)
            // See: https://hackrf.readthedocs.io/en/latest/faq.html#what-gain-controls-are-provided-by-hackrf
            match direction {
                Direction::Tx => Ok(vec!["IF".into()]),
                // TODO: add rest
                Direction::Rx => Ok(vec!["IF".into()]),
            }
        } else {
            Err(Error::ValueError)
        }
    }

    fn supports_agc(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
        if channel == 0 {
            Ok(false)
        } else {
            Err(Error::ValueError)
        }
    }

    fn enable_agc(&self, _direction: Direction, channel: usize, _agc: bool) -> Result<(), Error> {
        if channel == 0 {
            Err(Error::NotSupported)
        } else {
            Err(Error::ValueError)
        }
    }

    fn agc(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
        if channel == 0 {
            Err(Error::NotSupported)
        } else {
            Err(Error::ValueError)
        }
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.set_gain_element(direction, channel, "IF", gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.gain_element(direction, channel, "IF")
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.gain_element_range(direction, channel, "IF")
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let r = self.gain_range(direction, channel)?;
        if r.contains(gain) && name == "IF" {
            match direction {
                Direction::Tx => todo!(),
                Direction::Rx => {
                    let mut config = self.inner.rx_config.lock().unwrap();
                    config.lna_db = gain as u16;
                    Ok(())
                }
            }
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
        if channel == 0 && name == "IF" {
            match direction {
                Direction::Tx => todo!(),
                Direction::Rx => {
                    let config = self.inner.rx_config.lock().unwrap();
                    Ok(Some(config.lna_db as f64))
                }
            }
        } else {
            Err(Error::ValueError)
        }
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        // TODO: add support for other gains
        if channel == 0 && name == "IF" {
            match direction {
                Direction::Tx => Ok(Range::new(vec![RangeItem::Step(0.0, 47.0, 1.0)])),
                Direction::Rx => Ok(Range::new(vec![RangeItem::Step(0.0, 40.0, 8.0)])),
            }
        } else {
            Err(Error::ValueError)
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
        _direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        if channel == 0 {
            Ok(vec!["TUNER".to_string()])
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
        if channel == 0 && name == "TUNER" {
            // up to 7.25GHz
            Ok(Range::new(vec![RangeItem::Interval(0.0, 7_270_000_000.0)]))
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
        if channel == 0 && name == "TUNER" {
            self.with_config(direction, |config| Ok(config.frequency_hz as f64))
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
        if channel == 0
            && self
                .frequency_range(direction, channel)?
                .contains(frequency)
            && name == "TUNER"
        {
            self.with_config(direction, |config| {
                config.frequency_hz = frequency as u64;
                self.inner.dev.set_freq(frequency as u64)?;
                Ok(())
            })
        } else {
            Err(Error::ValueError)
        }
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        // NOTE: same state for both "directions" lets hope future sdr doesnt assume there are two
        // values here, should be fine since we told it we're not full duplex
        if channel == 0 {
            self.with_config(direction, |config| Ok(config.sample_rate_hz as f64))
        } else {
            Err(Error::ValueError)
        }
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        if channel == 0
            && self
                .get_sample_rate_range(direction, channel)?
                .contains(rate)
        {
            self.with_config(direction, |config| {
                // TODO: use sample rate div to enable lower effective sampling rate
                config.sample_rate_hz = rate as u32;
                config.sample_rate_div = 1;
            });
            Ok(())
        } else {
            Err(Error::ValueError)
        }
    }

    fn get_sample_rate_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(
                1_000_000.0,
                20_000_000.0,
            )]))
        } else {
            Err(Error::ValueError)
        }
    }
}
