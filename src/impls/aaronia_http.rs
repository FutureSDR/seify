//! Aaronia Spectran HTTP Client
use num_complex::Complex32;
use serde_json::json;
use serde_json::Value;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;
use ureq::Agent;

use crate::dev::DynDeviceBackend;
use crate::AgcControl;
use crate::AntennaControl;
use crate::Args;
use crate::BandwidthControl;
use crate::Capability;
use crate::ChannelInfo;
use crate::DeviceInfo;
use crate::Direction;
use crate::Direction::*;
use crate::Driver;
use crate::Error;
use crate::FrequencyControl;
use crate::GainControl;
use crate::Range;
use crate::RangeItem;
use crate::RxDevice;
use crate::SampleRateControl;
use crate::TxDevice;

/// Aaronia SpectranV6 driver, using the HTTP interface
#[derive(Clone)]
pub struct AaroniaHttp {
    url: String,
    tx_url: String,
    agent: Agent,
    f_offset: f64,
    tx_frequency: Arc<AtomicU64>,
    tx_sample_rate: Arc<AtomicU64>,
}

/// Aaronia SpectranV6 HTTP RX Streamer
pub struct RxStreamer {
    agent: Agent,
    url: String,
    items_left: usize,
    reader: Option<BufReader<Box<dyn Read + Send + Sync + 'static>>>,
}

/// expected maximum delay for the transfer of samples between host and rf hardware, used to set the transmit start time to an achievalble but close value; in seconds
const STREAMING_DELAY: f64 = 0.01; // 0.2 is too much, 0.001 too little

/// Aaronia SpectranV6 HTTP TX Streamer
pub struct TxStreamer {
    agent: Agent,
    url: String,
    frequency: Arc<AtomicU64>,
    sample_rate: Arc<AtomicU64>,
    last_transmission_end_time: f64,
}

impl AaroniaHttp {
    /// Try to connect to an Aaronia HTTP server interface
    ///
    /// Looks for a `url` argument or tries `http://localhost:54664` as the default.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let url = args
            .get::<String>("url")
            .unwrap_or_else(|_| String::from("http://localhost:54664"));
        let test_path = format!("{url}/info");

        let agent = Agent::new_with_defaults();
        let resp = match agent.get(&test_path).call() {
            Ok(r) => r,
            Err(e) => {
                if matches!(e, ureq::Error::ConnectionFailed)
                    && args.get::<String>("driver").is_ok()
                {
                    return Err(e.into());
                } else {
                    return Ok(Vec::new());
                }
            }
        };
        if resp.status() == 200 {
            let mut args = args.clone();
            args.merge(format!("driver=aaronia_http, url={url}").try_into()?);
            Ok(vec![args])
        } else {
            Ok(Vec::new())
        }
    }

    /// Create an Aaronia SpectranV6 HTTP Device
    ///
    /// Looks for a `url` argument or tries `http://localhost:54664` as the default.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let mut v = Self::probe(
            &args
                .try_into()
                .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?,
        )?;
        if v.is_empty() {
            Err(Error::DeviceNotFound)
        } else {
            let a = v.remove(0);

            let f_offset = a.get::<f64>("f_offset").unwrap_or(20e6);
            let url = a.get::<String>("url")?;
            let tx_url = a.get::<String>("tx_url").unwrap_or_else(|_| url.clone());

            Ok(Self {
                agent: Agent::new_with_defaults(),
                url,
                tx_url,
                f_offset,
                tx_frequency: Arc::new(AtomicU64::new(2_450_000_000)),
                tx_sample_rate: Arc::new(AtomicU64::new(1_000_000)),
            })
        }
    }
}

impl AaroniaHttp {
    fn config(&self) -> Result<Value, Error> {
        let url = format!("{}/remoteconfig", self.url);
        let s = self.agent.get(&url).call()?.body_mut().read_to_string()?;
        Ok(serde_json::from_str(&s)?)
    }

    fn get_element(&self, path: Vec<&str>) -> Result<Value, Error> {
        let config = self.config()?;
        let path_label = path.join(".");
        let mut element = config.get("config").ok_or_else(|| {
            Error::invalid_argument("aaronia_http", "remote config is missing config object")
        })?;

        for p in &path {
            let items = element
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    Error::invalid_argument(
                        "aaronia_http",
                        format!("remote config path {path_label} is missing items array"),
                    )
                })?;
            element = items
                .iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some(*p))
                .ok_or_else(|| {
                    Error::invalid_argument(
                        "aaronia_http",
                        format!("remote config path {path_label} is missing element {p}"),
                    )
                })?;
        }
        Ok(element.clone())
    }

    fn get_enum(&self, path: Vec<&str>) -> Result<(u64, String), Error> {
        let path_label = path.join(".");
        let element = self.get_element(path)?;
        let i = element
            .get("value")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                Error::invalid_argument(
                    "aaronia_http",
                    format!("remote config path {path_label} is missing enum value"),
                )
            })?;
        let values: Vec<&str> = element
            .get("values")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Error::invalid_argument(
                    "aaronia_http",
                    format!("remote config path {path_label} is missing enum values"),
                )
            })?
            .split(',')
            .collect();
        let value = values.get(i as usize).ok_or_else(|| {
            Error::invalid_argument(
                "aaronia_http",
                format!("remote config path {path_label} has enum value outside values list"),
            )
        })?;
        Ok((i, value.to_string()))
    }

    fn get_f64(&self, path: Vec<&str>) -> Result<f64, Error> {
        let path_label = path.join(".");
        let element = self.get_element(path)?;
        element.get("value").and_then(Value::as_f64).ok_or_else(|| {
            Error::invalid_argument(
                "aaronia_http",
                format!("remote config path {path_label} is missing numeric value"),
            )
        })
    }
    fn send_json(&self, json: Value) -> Result<(), Error> {
        self.agent
            .put(&format!("{}/remoteconfig", self.url))
            .send_json(json)?;

        Ok(())
    }
}

impl AaroniaHttp {
    fn driver(&self) -> Driver {
        Driver::AaroniaHttp
    }

    fn id(&self) -> Result<String, Error> {
        Ok(format!("driver=aaronia_http, url={}", self.url))
    }

    fn info(&self) -> Result<Args, Error> {
        format!("driver=aaronia_http, url={}", self.url).try_into()
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
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        match (direction, channel) {
            (Rx, 0) => Ok(vec!["RX1".to_string()]),
            (Rx, 1) => Ok(vec!["RX2".to_string()]),
            (Tx, 0) => Ok(vec!["TX1".to_string()]),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        match (direction, channel) {
            (Rx, 0) => Ok("RX1".to_string()),
            (Rx, 1) => Ok("RX2".to_string()),
            (Tx, 0) => Ok("TX1".to_string()),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        match (direction, channel, name) {
            (Rx, 0, "RX1") => Ok(()),
            (Rx, 1, "RX2") => Ok(()),
            (Tx, 0, "TX1") => Ok(()),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) | (Tx, 0) => Ok(Vec::new()),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(true),
            (Tx, 0) => Ok(false),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let json = json!({
                    "receiverName": "Block_Spectran_V6B_0",
                    "simpleconfig": {
                        "device": {
                            "gaincontrol": if agc { "peak" } else { "manual" }
                        }
                    }
                });
                self.send_json(json)
            }
            (Tx, 0) => Err(Error::unsupported(Capability::Agc)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let (_, s) = self.get_enum(vec![
                    "Block_Spectran_V6B_0",
                    "config",
                    "device",
                    "gaincontrol",
                ])?;
                Ok(s != "manual")
            }
            (Tx, 0) => Err(Error::unsupported(Capability::Agc)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let lvl = -gain - 8.0;
                let json = json!({
                        "receiverName": "Block_Spectran_V6B_0",
                        "simpleconfig": {
                        "main": {
                        "reflevel": lvl
                    }
                }
                });
                self.send_json(json)
            }
            (Tx, 0) => {
                let range = Range::new(vec![RangeItem::Interval(-100.0, 10.0)]);
                if !range.contains(gain) {
                    log::warn!("aaronia_http: gain out of range");
                    return Err(Error::out_of_range("gain", range, gain));
                }
                let json = json!({
                        "receiverName": "Block_Spectran_V6B_0",
                        "simpleconfig": {
                        "main": {
                        "transattn": gain
                    }
                }
                });
                self.send_json(json)
            }
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let lvl =
                    self.get_f64(vec!["Block_Spectran_V6B_0", "config", "main", "reflevel"])?;
                Ok(Some(-lvl - 8.0))
            }
            (Tx, 0) => Err(Error::unsupported(Capability::Gain)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(Range::new(vec![RangeItem::Interval(0.0, 30.0)])),
            (Tx, 0) => Err(Error::unsupported(Capability::Gain)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        _name: &str,
        _gain: f64,
    ) -> Result<(), Error> {
        match (direction, channel) {
            (Rx, 0 | 1) | (Tx, 0) => Err(Error::unsupported(Capability::Gain)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        _name: &str,
    ) -> Result<Option<f64>, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) | (Tx, 0) => Err(Error::unsupported(Capability::Gain)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        _name: &str,
    ) -> Result<Range, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) | (Tx, 0) => Err(Error::unsupported(Capability::Gain)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) | (Tx, 0) => Err(Error::unsupported(Capability::Frequency)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => self.get_f64(vec![
                "Block_IQDemodulator_0",
                "config",
                "main",
                "centerfreq",
            ]),
            (Tx, 0) => Ok(self.tx_frequency.load(Ordering::SeqCst) as f64),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let f = (frequency - self.f_offset).max(0.0);
                self.set_component_frequency(direction, channel, "RF", f)?;
                self.set_component_frequency(direction, channel, "DEMOD", self.f_offset)
            }
            (Tx, 0) => self.set_component_frequency(direction, channel, "RF", frequency),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(vec!["RF".to_string(), "DEMOD".to_string()]),
            (Tx, 0) => Ok(vec!["RF".to_string()]),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        match (direction, channel, name) {
            (Rx, 0 | 1, "RF" | "DEMOD") | (Tx, 0, "RF") => {
                Err(Error::unsupported(Capability::Frequency))
            }
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        match (direction, channel, name) {
            (Rx, 0 | 1, "DEMOD") => {
                let rf =
                    self.get_f64(vec!["Block_Spectran_V6B_0", "config", "main", "centerfreq"])?;
                let demod = self.get_f64(vec![
                    "Block_IQDemodulator_0",
                    "config",
                    "main",
                    "centerfreq",
                ])?;
                Ok(demod - rf)
            }
            (Rx, 0 | 1, "RF") => {
                self.get_f64(vec!["Block_Spectran_V6B_0", "config", "main", "centerfreq"])
            }
            (Tx, 0, "RF") => Ok(self.tx_frequency.load(Ordering::SeqCst) as f64),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        match (direction, channel, name) {
            (Rx, 0 | 1, "RF") => {
                let json = json!({
                    "receiverName": "Block_Spectran_V6B_0",
                    "simpleconfig": {
                        "main": {
                            "centerfreq": frequency
                        }
                    }
                });
                self.send_json(json)
            }
            (Rx, 0 | 1, "DEMOD") => {
                let rf =
                    self.get_f64(vec!["Block_Spectran_V6B_0", "config", "main", "centerfreq"])?;
                let json = json!({
                    "receiverName": "Block_IQDemodulator_0",
                    "simpleconfig": {
                        "main": {
                            "centerfreq": frequency + rf
                        }
                    }
                });
                self.send_json(json)
            }
            (Tx, 0, "RF") => {
                self.tx_frequency.store(frequency as u64, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => self.get_f64(vec![
                "Block_IQDemodulator_0",
                "config",
                "main",
                "samplerate",
            ]),
            (Tx, 0) => Ok(self.tx_sample_rate.load(Ordering::SeqCst) as f64),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let json = json!({
                        "receiverName": "Block_IQDemodulator_0",
                        "simpleconfig": {
                        "main": {
                        "samplerate": rate,
                        "spanfreq": rate
                    }
                }
                });
                self.send_json(json)
            }
            (Tx, 0) => {
                self.tx_sample_rate.store(rate as u64, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(Range::new(vec![RangeItem::Interval(0.0, 92.16e6)])),
            (Tx, 0) => Err(Error::unsupported(Capability::SampleRate)),
            _ => Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            )),
        }
    }

    fn bandwidth(&self, _direction: Direction, _channel: usize) -> Result<f64, Error> {
        Err(Error::unsupported(Capability::Bandwidth))
    }

    fn set_bandwidth(&self, _direction: Direction, _channel: usize, _bw: f64) -> Result<(), Error> {
        Err(Error::unsupported(Capability::Bandwidth))
    }

    fn get_bandwidth_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        Err(Error::unsupported(Capability::Bandwidth))
    }
}

impl DeviceInfo for AaroniaHttp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> Driver {
        AaroniaHttp::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        AaroniaHttp::id(self)
    }

    fn info(&self) -> Result<Args, Error> {
        AaroniaHttp::info(self)
    }
}

impl DynDeviceBackend for AaroniaHttp {
    fn channel_info(&self) -> Option<&dyn ChannelInfo> {
        Some(self)
    }

    fn rx_device(&self) -> Option<&dyn crate::dev::ErasedRxDevice> {
        Some(self)
    }

    fn tx_device(&self) -> Option<&dyn crate::dev::ErasedTxDevice> {
        Some(self)
    }

    fn antenna_control(&self) -> Option<&dyn AntennaControl> {
        Some(self)
    }

    fn agc_control(&self) -> Option<&dyn AgcControl> {
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

impl ChannelInfo for AaroniaHttp {
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        AaroniaHttp::num_channels(self, direction)
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        AaroniaHttp::full_duplex(self, direction, channel)
    }
}

impl RxDevice for AaroniaHttp {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels == [0] {
            Ok(RxStreamer {
                url: self.url.clone(),
                agent: self.agent.clone(),
                items_left: 0,
                reader: None,
            })
        } else {
            Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            ))
        }
    }
}

impl TxDevice for AaroniaHttp {
    type TxStreamer = TxStreamer;

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        if channels == [0] {
            Ok(TxStreamer {
                url: self.tx_url.clone(),
                agent: self.agent.clone(),
                frequency: self.tx_frequency.clone(),
                sample_rate: self.tx_sample_rate.clone(),
                last_transmission_end_time: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64(),
            })
        } else {
            Err(Error::invalid_argument(
                "aaronia_http",
                "invalid Aaronia HTTP argument",
            ))
        }
    }
}

impl AntennaControl for AaroniaHttp {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        AaroniaHttp::antennas(self, direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        AaroniaHttp::antenna(self, direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        AaroniaHttp::set_antenna(self, direction, channel, name)
    }
}

impl AgcControl for AaroniaHttp {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        AaroniaHttp::agc_available(self, direction, channel)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        AaroniaHttp::set_agc_enabled(self, direction, channel, agc)
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        AaroniaHttp::agc_enabled(self, direction, channel)
    }
}

impl GainControl for AaroniaHttp {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        AaroniaHttp::gain_elements(self, direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        AaroniaHttp::set_gain(self, direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        AaroniaHttp::gain(self, direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        AaroniaHttp::gain_range(self, direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        AaroniaHttp::set_gain_element(self, direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        AaroniaHttp::gain_element(self, direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AaroniaHttp::gain_element_range(self, direction, channel, name)
    }
}

impl FrequencyControl for AaroniaHttp {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        AaroniaHttp::frequency_range(self, direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AaroniaHttp::frequency(self, direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        AaroniaHttp::set_frequency(self, direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        AaroniaHttp::frequency_components(self, direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        AaroniaHttp::component_frequency_range(self, direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        AaroniaHttp::component_frequency(self, direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        AaroniaHttp::set_component_frequency(self, direction, channel, name, frequency)
    }
}

impl SampleRateControl for AaroniaHttp {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AaroniaHttp::sample_rate(self, direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        AaroniaHttp::set_sample_rate(self, direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        AaroniaHttp::get_sample_rate_range(self, direction, channel)
    }
}

impl BandwidthControl for AaroniaHttp {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        AaroniaHttp::bandwidth(self, direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        AaroniaHttp::set_bandwidth(self, direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        AaroniaHttp::get_bandwidth_range(self, direction, channel)
    }
}

impl RxStreamer {
    fn parse_header(&mut self) -> Result<(), Error> {
        let mut buf = Vec::with_capacity(512);
        let reader = self.reader.as_mut().ok_or(Error::StreamInactive)?;
        reader.read_until(10, &mut buf)?;
        let header: Value = serde_json::from_str(&String::from_utf8_lossy(&buf))?;
        reader.consume(1);

        let i = header
            .get("samples")
            .and_then(|x| x.to_string().parse::<usize>().ok())
            .ok_or_else(|| Error::invalid_argument("samples", "missing samples in JSON header"))?;

        self.items_left = i;
        Ok(())
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(65536)
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        let r = self
            .agent
            .get(&format!("{}/stream?format=float32", self.url))
            .call()?
            .into_body()
            .into_reader();
        self.reader = Some(BufReader::new(Box::new(r)));
        Ok(())
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        self.reader = None;
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;

        if self.items_left == 0 {
            self.parse_header()?;
        }

        let is = std::mem::size_of::<Complex32>();
        let n = std::cmp::min(self.items_left, buffers[0].len());

        let out =
            unsafe { std::slice::from_raw_parts_mut(buffers[0].as_mut_ptr() as *mut u8, n * is) };
        self.reader
            .as_mut()
            .ok_or(Error::StreamInactive)?
            .read_exact(&mut out[0..n * is])?;

        self.items_left -= n;

        Ok(n)
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(65536 * 8)
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
        at_ns: Option<i64>,
        end_burst: bool,
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;
        if at_ns.is_some() {
            return Err(Error::unsupported_reason(
                Capability::DriverOperation,
                "timed TX is not supported by Aaronia HTTP",
            ));
        }

        let frequency = self.frequency.load(Ordering::SeqCst) as f64;
        let sample_rate = self.sample_rate.load(Ordering::SeqCst) as f64;
        let len: usize = buffers[0].len();

        let start = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
            + STREAMING_DELAY;
        let num_streamable_samples = if start < self.last_transmission_end_time {
            // log::debug!("WARNING: cannot send immediately, expecting {}s delay.", self.last_transmission_end_time - (start - STREAMING_DELAY));
            let time_remaining_in_tx_queue = 1.0_f64 - (self.last_transmission_end_time - start);
            let num_streamable_samples_tmp = time_remaining_in_tx_queue * sample_rate;
            if num_streamable_samples_tmp <= 0.0 {
                // log::debug!("WARNING: stream start time lies more than one second in the future due to backed up TX queue.");
                // tx queue fully backed up
                return Ok(0);
            } else if end_burst && (num_streamable_samples_tmp as usize) < len {
                // not enough space in tx queue to send burst in one go -> return and retry later
                // log::debug!("WARNING: cannot send burst while assuring less than 1s streaming delay.");
                return Err(Error::Busy);
            } else if (num_streamable_samples_tmp as usize) < len {
                // log::debug!("WARNING: tx queue running full, sending only a subset of samples ({}/{}).", num_streamable_samples_tmp, len);
                num_streamable_samples_tmp as usize
            } else {
                // log::debug!("WARNING: tx queue starting to run full.");
                len
            }
        } else {
            len
        };
        let start = start.max(self.last_transmission_end_time);
        let stop = start + num_streamable_samples as f64 / sample_rate;
        self.last_transmission_end_time = stop + 1.0_f64 / sample_rate; // use one sample spacing between queued requests

        let samples = unsafe {
            std::slice::from_raw_parts(
                buffers[0].as_ptr() as *const f32,
                num_streamable_samples * 2,
            )
        };

        // log::debug!(
        //     "sending {}{} samples with delay of {}s",
        //     if end_burst { "burst of " } else { "" },
        //     num_streamable_samples,
        //     start
        //         - SystemTime::now()
        //             .duration_since(SystemTime::UNIX_EPOCH)
        //             .unwrap()
        //             .as_secs_f64()
        // );

        let j = json!({
            "startTime": start,
            "endTime": stop,
            "startFrequency": frequency - sample_rate / 2.0,
            "endFrequency": frequency + sample_rate / 2.0,
            // parameter "stepFrequency": sample_rate, not required for upload/tx, used for subsampling in rx
            "minPower": -2,
            "maxPower": 2,
            "sampleSize": 2,
            "sampleDepth": 1,
            "unit": "volt",
            "payload": "iq",
            // do not set "flush": true, else it will drop all preceding samples still in the queue
            "push": true,
            // parameter "format": "json" or "f32" not necessary for upload/tx, used to request specific format in rx
            "samples": samples,
        });

        self.agent
            .post(&format!("{}/sample", self.url))
            .send_json(j)?;

        Ok(num_streamable_samples)
    }

    fn write_all(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        crate::streamer::expect_buffer_count(buffers.len(), 1)?;

        let mut offset = 0;
        while offset < buffers[0].len() {
            let samples = &buffers[0][offset..];
            let written = self.write(
                &[samples],
                if offset == 0 { at_ns } else { None },
                end_burst && offset + samples.len() == buffers[0].len(),
                timeout_us,
            )?;
            if written == 0 {
                return Err(Error::Busy);
            }
            offset += written;
        }
        Ok(())
    }
}
