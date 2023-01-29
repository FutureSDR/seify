//! Aaronia Spectran HTTP Client
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
use async_task::Task;
use futures::Future;
use futures::StreamExt;
use futures::TryStreamExt;
use hyper::body::Buf;
use hyper::body::Bytes;
// use hyper::client::connect::Connect;
use hyper::client::connect::HttpConnector;
use hyper::Request;
use hyper::{Body, Client, Uri};
use log::debug;
use num_complex::Complex32;
use once_cell::sync::OnceCell;
use serde_json::json;
use serde_json::Number;
use serde_json::Value;
use tokio::runtime::Builder;
use tokio::runtime::Handle;
use tokio::runtime::Runtime;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Direction::*;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RangeItem;

static RUNTIME: OnceCell<Runtime> = OnceCell::new();

// Wraps a provided scheduler.
#[derive(Clone)]
struct MyExecutor<E: Executor>(E);

/// HTTP Connect implementation for the async runtime, used by the scheduler.
pub trait Connect: hyper::client::connect::Connect + Clone + Send + Sync + 'static {}

impl<E: Executor> MyExecutor<E> {
    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.0.block_on(future)
    }
}

/// Async Executor for Hyper to spawn Tasks
pub trait Executor: Clone + Send + Sync + 'static {
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static)
        -> Task<T>;
    fn block_on<T>(&self, future: impl Future<Output = T>) -> T;
}

impl<F, E> hyper::rt::Executor<F> for MyExecutor<E>
where
    E: Executor,
    F: Future + Send + 'static,
{
    fn execute(&self, fut: F) {
        self.0.spawn(async { drop(fut.await) }).detach();
    }
}

impl Executor for tokio::runtime::Handle {
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static)
        -> Task<T> {
        self.spawn(future)
    }

    fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.block_on(future)
    }
}

/// Aaronia SpectranV6 driver, using the HTTP interface
#[derive(Clone)]
pub struct AaroniaHttp<E: Executor, C: Connect> {
    url: String,
    executor: MyExecutor<E>,
    client: Client<C, Body>,
    f_offset: f64,
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
pub struct TxStreamer<E: Executor> {
    executor: MyExecutor<E>,
}

impl<E: Executor, C: Connect + Clone> AaroniaHttp<E, C> {
    /// Try to connect to an Aaronia HTTP server interface
    ///
    /// Looks for a `url` argument or tries `http://localhost:54664` as the default.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let rt = RUNTIME.get_or_try_init(Runtime::new)?;

        rt.block_on(async {
            let url = args
                .get::<String>("url")
                .unwrap_or_else(|_| String::from("http://localhost:54664"));
            let test_path = format!("{url}/info").parse().or(Err(Error::ValueError))?;

            let client = Client::new();
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
                Ok(vec![format!("driver=aaronia_http, url={url}").try_into()?])
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
        let executor = MyExecutor(executor);
        let mut v = Self::probe(&args.try_into().or(Err(Error::ValueError))?)?;
        if v.is_empty() {
            Err(Error::NotFound)
        } else {
            // let rt = RUNTIME.get_or_try_init(Runtime::new)?;
            let a = v.remove(0);

            let f_offset = a.get::<f64>("f_offset").unwrap_or(20e6);

            Ok(Self {
                client: Client::builder()
                    .executor(executor.clone())
                    .build(connector),
                executor,
                url: a.get::<String>("url")?,
                f_offset,
            })
        }
    }

    fn config(&self) -> Result<Value, Error> {
        self.executor.block_on(async {
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
            .block_on(async { self.client.request(req).await.or(Err(Error::Io)) })?;

        Ok(())
    }
}

impl<E: Executor + Send + 'static, C: Connect + Send + 'static> DeviceTrait for AaroniaHttp<E, C> {
    type RxStreamer = RxStreamer<E, C>;
    type TxStreamer = TxStreamer<E>;

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

    fn tx_streamer(&self, channels: &[usize], args: Args) -> Result<Self::TxStreamer, Error> {
        todo!()
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

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        todo!()
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        let (i, s) = dbg!(self.get_enum(vec![
            "Block_Spectran_V6B_0",
            "config",
            "device",
            "gaincontrol"
        ])?);
        if s == "manual" {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
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

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        let lvl = self.get_f64(vec!["Block_Spectran_V6B_0", "config", "main", "reflevel"])?;
        Ok(Some(-lvl - 8.0))
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        Ok(Range::new(vec![RangeItem::Interval(0.0, 30.0)]))
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
    ) -> Result<Option<f64>, Error> {
        todo!()
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        todo!()
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        todo!()
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.get_f64(vec![
            "Block_IQDemodulator_0",
            "config",
            "main",
            "centerfreq",
        ])
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
                self.set_component_frequency(direction, channel, "DEMOD", frequency)
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
        Ok(vec!["RF".to_string(), "DEMOD".to_string()])
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        todo!()
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        self.get_f64(vec![
            "Block_IQDemodulator_0",
            "config",
            "main",
            "centerfreq",
        ])
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        let json = match (direction, channel, name) {
            (Rx, 0 | 1, "RF") => {
                json!({
                    "receiverName": "Block_Spectran_V6B_0",
                    "simpleconfig": {
                        "main": {
                            "centerfreq": frequency
                        }
                    }
                })
            }
            (Rx, 0 | 1, "DEMOD") => {
                json!({
                    "receiverName": "Block_IQDemodulator_0",
                    "simpleconfig": {
                        "main": {
                            "centerfreq": frequency
                        }
                    }
                })
            }
            (Tx, 0, "RF") => {
                json!({
                    "receiverName": "Block_Spectran_V6B_0",
                    "simpleconfig": {
                        "main": {
                            "centerfreq": frequency
                        }
                    }
                })
            }
            _ => return Err(Error::ValueError),
        };

        self.send_json(json)
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.get_f64(vec![
            "Block_IQDemodulator_0",
            "config",
            "main",
            "samplerate",
        ])
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
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

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        let stream = self.executor.block_on(async {
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

    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        self.stream = None;
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        timeout_us: i64,
    ) -> Result<usize, Error> {
        self.executor.clone().block_on(async {
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

impl<E: Executor + Send> crate::TxStreamer for TxStreamer<E> {
    fn mtu(&self) -> Result<usize, Error> {
        todo!()
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
