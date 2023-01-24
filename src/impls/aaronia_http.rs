#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
use futures::StreamExt;
use futures::TryStreamExt;
use hyper::body::Buf;
use hyper::body::Bytes;
use hyper::Request;
use hyper::{Body, Client, Uri};
use log::debug;
use num_complex::Complex32;
use once_cell::sync::OnceCell;
use serde_json::Value;
use tokio::runtime::Builder;
use tokio::runtime::Handle;
use tokio::runtime::Runtime;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RangeItem;

static RUNTIME: OnceCell<Runtime> = OnceCell::new();

#[derive(Clone)]
pub struct AaroniaHttp {
    url: String,
    runtime: Handle,
}

pub struct RxStreamer {
    runtime: Handle,
    url: String,
    stream: Option<futures::stream::IntoStream<Body>>,
    buf: Bytes,
    items_left: usize,
}

pub struct TxStreamer {
    runtime: Handle,
}

impl AaroniaHttp {
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let rt = RUNTIME.get_or_try_init(|| Runtime::new())?;

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
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let mut v = Self::probe(&args.try_into().or(Err(Error::ValueError))?)?;
        if v.is_empty() {
            Err(Error::NotFound)
        } else {
            let rt = RUNTIME.get_or_try_init(|| Runtime::new())?;
            let a = v.remove(0);
            Ok(Self {
                runtime: rt.handle().clone(),
                url: a.get::<String>("url")?,
            })
        }
    }
}

impl DeviceTrait for AaroniaHttp {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxStreamer;

    fn as_any(&self) -> &dyn std::any::Any {
        todo!()
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        todo!()
    }

    fn driver(&self) -> Driver {
        Driver::AaroniaHttp
    }

    fn id(&self) -> Result<String, Error> {
        Ok(format!("driver=aarnia_http, url={}", self.url))
    }

    fn info(&self) -> Result<Args, Error> {
        Ok(format!("driver=aarnia_http, url={}", self.url).try_into()?)
    }

    fn num_channels(&self, _direction: Direction) -> Result<usize, Error> {
        Ok(1)
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        if channel == 0 {
            Ok(true)
        } else {
            Err(Error::ValueError)
        }
    }

    fn rx_stream(&self, channels: &[usize]) -> Result<Self::RxStreamer, Error> {
        self.rx_stream_with_args(channels, Args::new())
    }

    fn rx_stream_with_args(
        &self,
        channels: &[usize],
        _args: Args,
    ) -> Result<Self::RxStreamer, Error> {
        if channels == [0] {
            Ok(RxStreamer {
                url: self.url.clone(),
                runtime: self.runtime.clone(),
                stream: None,
                buf: Bytes::new(),
                items_left: 0,
            })
        } else {
            Err(Error::ValueError)
        }
    }

    fn tx_stream(&self, channels: &[usize]) -> Result<Self::TxStreamer, Error> {
        todo!()
    }

    fn tx_stream_with_args(
        &self,
        channels: &[usize],
        args: Args,
    ) -> Result<Self::TxStreamer, Error> {
        todo!()
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        todo!()
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        todo!()
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        todo!()
    }

    fn suports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        todo!()
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        todo!()
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        todo!()
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        todo!()
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        todo!()
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
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
        todo!()
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
    ) -> Result<(), Error> {
        let data = r#"{
          "request" : 1,
          "config" : {
            "type" : "group",
            "name" : "Block_IQDemodulator_0",
            "items" : [{
              "type" : "group",
              "name" : "config",
              "items" : [{
                "type" : "group",
                "name" : "main",
                "items" : [{
                  "type" : "float",
                  "name" : "centerfreq",
                  "value" : 5510000000
                }]
              }]
            }]
          }
        }"#;

        let v: Value = dbg!(serde_json::from_str(data)).or(Err(Error::ValueError))?;

        let req = Request::put(format!("{}/remoteconfig", self.url))
            .body(Body::from(serde_json::to_vec(&v).or(Err(Error::ValueError))?))
            .or(Err(Error::ValueError))?;

        self.runtime
            .block_on(async { Client::new().request(req).await.or(Err(Error::Io)) })?;

        Ok(())
    }

    fn frequency_components(
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
    ) -> Result<Range, Error> {
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

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        todo!()
    }
}

impl RxStreamer {
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

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(65536)
    }

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        let stream = self.runtime.block_on(async {
            Ok::<futures::stream::IntoStream<Body>, Error>(
                Client::new()
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
        self.runtime.clone().block_on(async {
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

impl crate::TxStreamer for TxStreamer {
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
