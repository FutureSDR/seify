//! Aaronia Spectran HTTP Client
use futures::StreamExt;
use futures::TryStreamExt;
use hyper::body::Buf;
use hyper::body::Bytes;
use hyper::Request;
use hyper::{Body, Client};
use num_complex::Complex32;
use serde_json::json;
use serde_json::Value;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

use crate::web::MyExecutor;
use crate::Args;
use crate::Connect;
use crate::DefaultConnector;
use crate::DefaultExecutor;
use crate::DeviceTrait;
use crate::Direction;
use crate::Direction::*;
use crate::Driver;
use crate::Error;
use crate::Executor;
use crate::Range;
use crate::RangeItem;

/// Aaronia SpectranV6 driver, using the HTTP interface
#[derive(Clone)]
pub struct AaroniaHttp<E: Executor, C: Connect> {
    url: String,
    executor: MyExecutor<E>,
    client: Client<C, Body>,
    f_offset: f64,
    tx_frequency: Arc<AtomicU64>,
    tx_sample_rate: Arc<AtomicU64>,
}

/// Aaronia SpectranV6 HTTP RX Streamer
pub struct RxStreamer<E: Executor, C: Connect> {
    executor: MyExecutor<E>,
    url: String,
    stream: Option<futures::stream::IntoStream<Body>>,
    buf: Bytes,
    items_left: usize,
    client: Client<C, Body>,
}

/// Aaronia SpectranV6 HTTP TX Streamer
pub struct TxStreamer<E: Executor, C: Connect> {
    client: Client<C, Body>,
    url: String,
    executor: MyExecutor<E>,
    frequency: Arc<AtomicU64>,
    sample_rate: Arc<AtomicU64>,
}

impl AaroniaHttp<DefaultExecutor, DefaultConnector> {
    /// Try to connect to an Aaronia HTTP server interface
    ///
    /// Looks for a `url` argument or tries `http://localhost:54664` as the default.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        Self::probe_with_runtime(
            args,
            DefaultExecutor::default(),
            DefaultConnector::default(),
        )
    }

    /// Create an Aaronia SpectranV6 HTTP Device
    ///
    /// Looks for a `url` argument or tries `http://localhost:54664` as the default.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        Self::open_with_runtime(
            args,
            DefaultExecutor::default(),
            DefaultConnector::default(),
        )
    }
}

impl<E: Executor, C: Connect> AaroniaHttp<E, C> {
    /// Try to connect to an Aaronia HTTP server interface
    ///
    /// Looks for a `url` argument or tries `http://localhost:54664` as the default.
    pub fn probe_with_runtime(args: &Args, executor: E, connector: C) -> Result<Vec<Args>, Error> {
        executor.block_on(async {
            let url = args
                .get::<String>("url")
                .unwrap_or_else(|_| String::from("http://localhost:54664"));
            let test_path = format!("{url}/info").parse().or(Err(Error::ValueError))?;

            let client: hyper::Client<C, Body> = Client::builder()
                .executor(MyExecutor(executor.clone()))
                .build(connector);
            let resp = match client.get(test_path).await {
                Ok(r) => r,
                Err(e) => {
                    if e.is_connect() && args.get::<String>("driver").is_ok() {
                        return Err(Error::Io);
                    } else {
                        return Ok(Vec::new());
                    }
                }
            };
            if resp.status().is_success() {
                let mut args = args.clone();
                args.merge(format!("driver=aaronia_http, url={url}").try_into()?);
                Ok(vec![args])
            } else {
                Ok(Vec::new())
            }
        })
    }
    /// Create an Aaronia SpectranV6 HTTP Device
    ///
    /// Looks for a `url` argument or tries `http://localhost:54664` as the default.
    // pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
    //     let mut v = Self::probe(&args.try_into().or(Err(Error::ValueError))?)?;
    //     if v.is_empty() {
    //         Err(Error::NotFound)
    //     } else {
    //         let rt = RUNTIME.get_or_try_init(Runtime::new)?;
    //         let a = v.remove(0);
    //
    //         let f_offset = a.get::<f64>("f_offset").unwrap_or(20e6);
    //
    //         Ok(Self {
    //             client: Client::new(),
    //             runtime: rt.handle().clone(),
    //             url: a.get::<String>("url")?,
    //             f_offset,
    //         })
    //     }
    // }
    /// Create an Aaronia SpectranV6 HTTP Device
    ///
    /// Looks for a `url` argument or tries `http://localhost:54664` as the default.
    pub fn open_with_runtime<A: TryInto<Args>>(
        args: A,
        executor: E,
        connector: C,
    ) -> Result<Self, Error> {
        let mut v = Self::probe_with_runtime(
            &args.try_into().or(Err(Error::ValueError))?,
            executor.clone(),
            connector.clone(),
        )?;
        if v.is_empty() {
            Err(Error::NotFound)
        } else {
            let a = v.remove(0);

            let f_offset = a.get::<f64>("f_offset").unwrap_or(20e6);

            Ok(Self {
                client: Client::builder()
                    .executor(MyExecutor(executor.clone()))
                    .build(connector),
                executor: MyExecutor(executor),
                url: a.get::<String>("url")?,
                f_offset,
                tx_frequency: Arc::new(AtomicU64::new(2_450_000_000)),
                tx_sample_rate: Arc::new(AtomicU64::new(1_000_000)),
            })
        }
    }

    fn config(&self) -> Result<Value, Error> {
        self.executor.0.block_on(async {
            let url = format!("{}/remoteconfig", self.url)
                .parse()
                .or(Err(Error::ValueError))?;
            let body = self.client.get(url).await.or(Err(Error::Io))?.into_body();
            let bytes = hyper::body::to_bytes(body).await.or(Err(Error::Io))?;
            serde_json::from_slice(&bytes).or(Err(Error::ValueError))
        })
    }

    fn get_element(&self, path: Vec<&str>) -> Result<Value, Error> {
        let config = self.config()?;
        let mut element = &config["config"];
        for p in path {
            for i in element["items"].as_array().unwrap() {
                if i["name"].as_str().unwrap() == p {
                    element = i;
                }
            }
        }
        Ok(element.clone())
    }

    fn get_enum(&self, path: Vec<&str>) -> Result<(u64, String), Error> {
        let element = self.get_element(path)?;
        let i = dbg!(&element["value"]).as_u64().unwrap();
        let v: Vec<&str> = element["values"].as_str().unwrap().split(',').collect();
        Ok((i, v[i as usize].to_string()))
    }

    fn get_f64(&self, path: Vec<&str>) -> Result<f64, Error> {
        let element = self.get_element(path)?;
        Ok(element["value"].as_f64().unwrap())
    }
    fn send_json(&self, json: Value) -> Result<(), Error> {
        let req = Request::put(format!("{}/remoteconfig", self.url))
            .body(Body::from(
                serde_json::to_vec(&json).or(Err(Error::ValueError))?,
            ))
            .or(Err(Error::Io))?;

        self.executor
            .0
            .block_on(async { self.client.request(req).await.or(Err(Error::Io)) })?;

        Ok(())
    }
}

impl<E: Executor + Send + 'static, C: Connect + Send + 'static> DeviceTrait for AaroniaHttp<E, C> {
    type RxStreamer = RxStreamer<E, C>;
    type TxStreamer = TxStreamer<E, C>;

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> Driver {
        Driver::AaroniaHttp
    }

    fn id(&self) -> Result<String, Error> {
        Ok(format!("driver=aarnia_http, url={}", self.url))
    }

    fn info(&self) -> Result<Args, Error> {
        format!("driver=aarnia_http, url={}", self.url).try_into()
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

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels == [0] {
            Ok(RxStreamer {
                url: self.url.clone(),
                executor: self.executor.clone(),
                stream: None,
                buf: Bytes::new(),
                items_left: 0,
                client: self.client.clone(),
            })
        } else {
            Err(Error::ValueError)
        }
    }

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        if channels == [0] {
            Ok(TxStreamer {
                url: self.url.clone(),
                executor: self.executor.clone(),
                client: self.client.clone(),
                frequency: self.tx_frequency.clone(),
                sample_rate: self.tx_sample_rate.clone(),
            })
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
            _ => Err(Error::ValueError),
        }
    }

    fn enable_agc(&self, _direction: Direction, _channel: usize, _agc: bool) -> Result<(), Error> {
        todo!()
    }

    fn agc(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        let (_, s) = self.get_enum(vec![
            "Block_Spectran_V6B_0",
            "config",
            "device",
            "gaincontrol",
        ])?;
        if s == "manual" {
            Ok(false)
        } else {
            Ok(true)
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
                if gain < -100.0 || gain > 10.0 {
                    return Err(Error::OutOfRange);
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
            _ => Err(Error::ValueError),
        }
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let lvl =
                    self.get_f64(vec!["Block_Spectran_V6B_0", "config", "main", "reflevel"])?;
                Ok(Some(-lvl - 8.0))
            }
            _ => {
                todo!()
            }
        }
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(Range::new(vec![RangeItem::Interval(0.0, 30.0)])),
            _ => todo!(),
        }
    }

    fn set_gain_element(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
        _gain: f64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn gain_element(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<Option<f64>, Error> {
        todo!()
    }

    fn gain_element_range(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<Range, Error> {
        todo!()
    }

    fn frequency_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        todo!()
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
        match (direction, channel) {
            (Rx, 0 | 1) => {
                let f = (frequency - self.f_offset).max(0.0);
                self.set_component_frequency(direction, channel, "RF", f)?;
                self.set_component_frequency(direction, channel, "DEMOD", self.f_offset)
            }
            (Tx, 0) => self.set_component_frequency(direction, channel, "RF", frequency),
            _ => Err(Error::ValueError),
        }
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(vec!["RF".to_string(), "DEMOD".to_string()]),
            _ => todo!(),
        }
    }

    fn component_frequency_range(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<Range, Error> {
        todo!()
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
            _ => todo!(),
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
            _ => return Err(Error::ValueError),
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
            _ => Err(Error::ValueError),
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
            },
            (Tx, 0) => {
                self.tx_sample_rate.store(rate as u64, Ordering::SeqCst);
                Ok(())
            },
            _ => Err(Error::ValueError),
        }
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        match (direction, channel) {
            (Rx, 0 | 1) => Ok(Range::new(vec![RangeItem::Interval(0.0, 92.16e6)])),
            (Tx, 0) => todo!(),
            _ => Err(Error::ValueError),
        }
    }
}

impl<E: Executor, C: Connect> RxStreamer<E, C> {
    fn parse_header(&mut self) -> Result<(), Error> {
        if let Some(i) = self.buf.iter().position(|&b| b == 10) {
            let header: Value = serde_json::from_str(&String::from_utf8_lossy(&self.buf[0..i]))
                .or(Err(Error::Io))?;
            if self.buf.len() > i + 2 {
                self.buf.advance(i + 2);
            } else {
                self.buf = Bytes::new();
            }
            let i = header
                .get("samples")
                .and_then(|x| x.to_string().parse::<usize>().ok())
                .ok_or(Error::Io)?;
            self.items_left = i;
        }
        Ok(())
    }

    async fn get_data(&mut self) -> Result<(), Error> {
        let b = self
            .stream
            .as_mut()
            .unwrap()
            .next()
            .await
            .ok_or(Error::Io)?
            .or(Err(Error::Io))?;
        self.buf = [std::mem::take(&mut self.buf), b].concat().into();
        Ok(())
    }
}

impl<E: Executor + Send, C: Connect + Send> crate::RxStreamer for RxStreamer<E, C> {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(65536)
    }

    fn activate(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        let stream = self.executor.0.block_on(async {
            Ok::<futures::stream::IntoStream<Body>, Error>(
                self.client
                    .get(
                        format!("{}/stream?format=float32", self.url)
                            .parse()
                            .or(Err(Error::ValueError))?,
                    )
                    .await
                    .or(Err(Error::Io))?
                    .into_body()
                    .into_stream(),
            )
        })?;

        self.stream = Some(stream);
        Ok(())
    }

    fn deactivate(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        self.stream = None;
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        self.executor.0.clone().block_on(async {
            if self.items_left == 0 {
                self.parse_header()?;
                while self.items_left == 0 {
                    self.get_data().await?;
                    self.parse_header()?
                }
            }

            let is = std::mem::size_of::<Complex32>();
            let n = std::cmp::min(self.buf.len() / is, buffers[0].len());
            let n = std::cmp::min(n, self.items_left);

            unsafe {
                let out =
                    std::slice::from_raw_parts_mut(buffers[0].as_mut_ptr() as *mut u8, n * is);
                out[0..n * is].copy_from_slice(&self.buf[0..n * is]);
            }

            if n == self.buf.len() / is {
                self.buf.advance(n * is);
                self.get_data().await?;
            } else {
                self.buf.advance(n * is);
            }

            self.items_left -= n;

            Ok(n)
        })
    }
}

impl<E: Executor + Send, C: Connect> crate::TxStreamer for TxStreamer<E, C> {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(65536 * 8)
    }

    fn activate(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn deactivate(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);
        debug_assert_eq!(at_ns, None);

        if !end_burst {
            return Ok(0);
        }

        let frequency = self.frequency.load(Ordering::SeqCst) as f64;
        let sample_rate = self.sample_rate.load(Ordering::SeqCst) as f64;

        let start = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
            + 0.8;
        let len = buffers[0].len();
        let stop = start + len as f64 / sample_rate;

        let samples =
            unsafe { std::slice::from_raw_parts(buffers[0].as_ptr() as *const f32, len * 2) };

        println!("sending json -- size {}   frequency {}   sample_rate {}", samples.len() / 2, frequency, sample_rate);

        let j = json!({
            "startTime": start,
            "endTime": stop,
            "startFrequency": frequency - sample_rate / 2.0,
            "endFrequency": frequency + sample_rate / 2.0,
            "payload": "iq",
            "flush": true,
            "push": true,
            "format": "json",
            "samples": samples,
        });

        let req = Request::post(format!("{}/sample", self.url))
            .body(Body::from(j.to_string()))
            .or(Err(Error::Io))?;

        self.executor
            .0
            .block_on(self.client.request(req))
            .or(Err(Error::Io))?;

        Ok(len)
    }

    fn write_all(
        &mut self,
        _buffers: &[&[num_complex::Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<(), Error> {
        unimplemented!()
    }
}
