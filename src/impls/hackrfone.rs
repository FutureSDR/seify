use std::sync::{Arc, Mutex};

use num_complex::Complex32;
use seify_hackrfone::Config;

use crate::{
    dev::DynDeviceBackend, AntennaControl, Args, BandwidthControl, Capability, ChannelInfo,
    DeviceInfo, Direction, Error, FrequencyControl, GainControl, Range, RangeItem, RxDevice,
    SampleRateControl,
};

/// HackRF One device backend.
pub struct HackRfOne {
    inner: Arc<HackRfInner>,
}

const MTU: usize = 64 * 1024;

impl HackRfOne {
    /// Return descriptors for detected HackRF One devices.
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

    /// Open a HackRF One device from arguments.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args: Args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;

        // TODO(troy):
        // re-enable once new version of nusb is published: https://github.com/kevinmehall/nusb/issues/84
        /*
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
        */

        let bus_number = args.get("bus_number");
        let address = args.get("address");
        let dev = match (bus_number, address) {
            (Ok(bus_number), Ok(address)) => {
                seify_hackrfone::HackRf::open_bus(bus_number, address)?
            }
            (Err(Error::MissingArgument { .. }), Err(Error::MissingArgument { .. })) => {
                log::debug!("Opening first hackrf device");
                seify_hackrfone::HackRf::open_first()?
            }
            (bus_number, address) => {
                log::warn!("HackRfOne::open received invalid args: bus_number: {bus_number:?}, address: {address:?}");
                return Err(Error::invalid_argument("args", "invalid HackRF selector"));
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

    /// Mutate the cached RX or TX configuration through a closure.
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

/// HackRF One receive streamer.
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

        let _ = self.stream.take().ok_or(Error::StreamInactive)?;
        self.inner.dev.stop_rx()?;
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;

        if buffers[0].is_empty() {
            return Ok(0);
        }
        let buf = self
            .stream
            .as_mut()
            .ok_or(Error::StreamInactive)?
            .read_sync(buffers[0].len())?;

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

impl HackRfOne {
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

    fn num_channels(&self, direction: crate::Direction) -> Result<usize, Error> {
        Ok(match direction {
            Direction::Rx => 1,
            Direction::Tx => 0,
        })
    }

    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(false)
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
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
        }
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        if channel == 0 {
            if direction == Direction::Rx && name == "RX"
                || direction == Direction::Tx && name == "TX"
            {
                Ok(())
            } else {
                Err(Error::unsupported(Capability::Antenna))
            }
        } else {
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
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
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
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
                Direction::Tx => Err(Error::unsupported(Capability::Gain)),
                Direction::Rx => {
                    let mut config = self.inner.rx_config.lock().unwrap();
                    config.lna_db = gain as u16;
                    Ok(())
                }
            }
        } else {
            log::warn!("Gain out of range");
            Err(Error::out_of_range("gain", r, gain))
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
                Direction::Tx => Err(Error::unsupported(Capability::Gain)),
                Direction::Rx => {
                    let config = self.inner.rx_config.lock().unwrap();
                    Ok(Some(config.lna_db as f64))
                }
            }
        } else {
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
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
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
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
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
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
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
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
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
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
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
        }
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        // NOTE: same state for both "directions" lets hope future sdr doesnt assume there are two
        // values here, should be fine since we told it we're not full duplex
        if channel == 0 {
            self.with_config(direction, |config| Ok(config.sample_rate_hz as f64))
        } else {
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
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
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
        }
    }

    fn get_sample_rate_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(
                1_000_000.0,
                20_000_000.0,
            )]))
        } else {
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
        }
    }

    fn bandwidth(&self, _direction: Direction, _channel: usize) -> Result<f64, Error> {
        Err(Error::unsupported(Capability::Bandwidth))
    }

    fn set_bandwidth(&self, _direction: Direction, _channel: usize, bw: f64) -> Result<(), Error> {
        Ok(self.inner.dev.set_baseband_filter_bandwidth(bw as _)?)
    }

    fn get_bandwidth_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        Err(Error::unsupported(Capability::Bandwidth))
    }
}

impl DeviceInfo for HackRfOne {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> crate::Driver {
        HackRfOne::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        HackRfOne::id(self)
    }

    fn info(&self) -> Result<crate::Args, Error> {
        HackRfOne::info(self)
    }
}

impl DynDeviceBackend for HackRfOne {
    fn channel_info(&self) -> Option<&dyn ChannelInfo> {
        Some(self)
    }

    fn rx_device(&self) -> Option<&dyn crate::dev::ErasedRxDevice> {
        Some(self)
    }

    fn antenna_control(&self) -> Option<&dyn AntennaControl> {
        Some(self)
    }

    fn gain_control(&self) -> Option<&dyn GainControl> {
        Some(self)
    }

    fn frequency_control(&self) -> Option<&dyn FrequencyControl> {
        Some(self)
    }

    fn sample_rate_control(&self) -> Option<&dyn SampleRateControl> {
        Some(self)
    }

    fn bandwidth_control(&self) -> Option<&dyn BandwidthControl> {
        Some(self)
    }
}

impl ChannelInfo for HackRfOne {
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        HackRfOne::num_channels(self, direction)
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        HackRfOne::full_duplex(self, direction, channel)
    }
}

impl RxDevice for HackRfOne {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            Err(Error::invalid_argument("hackrf", "invalid HackRF argument"))
        } else {
            Ok(RxStreamer::new(Arc::clone(&self.inner)))
        }
    }
}

impl AntennaControl for HackRfOne {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        HackRfOne::antennas(self, direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        HackRfOne::antenna(self, direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        HackRfOne::set_antenna(self, direction, channel, name)
    }
}

impl GainControl for HackRfOne {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        HackRfOne::gain_elements(self, direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        HackRfOne::set_gain(self, direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        HackRfOne::gain(self, direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HackRfOne::gain_range(self, direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        HackRfOne::set_gain_element(self, direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        HackRfOne::gain_element(self, direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        HackRfOne::gain_element_range(self, direction, channel, name)
    }
}

impl FrequencyControl for HackRfOne {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HackRfOne::frequency_range(self, direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        HackRfOne::frequency(self, direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        HackRfOne::set_frequency(self, direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        HackRfOne::frequency_components(self, direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        HackRfOne::component_frequency_range(self, direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        HackRfOne::component_frequency(self, direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        HackRfOne::set_component_frequency(self, direction, channel, name, frequency)
    }
}

impl SampleRateControl for HackRfOne {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        HackRfOne::sample_rate(self, direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        HackRfOne::set_sample_rate(self, direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HackRfOne::get_sample_rate_range(self, direction, channel)
    }
}

impl BandwidthControl for HackRfOne {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        HackRfOne::bandwidth(self, direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        HackRfOne::set_bandwidth(self, direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        HackRfOne::get_bandwidth_range(self, direction, channel)
    }
}
