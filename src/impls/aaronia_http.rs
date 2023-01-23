#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
use futures::StreamExt;
use futures::TryStreamExt;
use hyper::body::Buf;
use hyper::body::Bytes;
use hyper::{Body, Client, Uri};
use num_complex::Complex32;
use tokio::runtime::Builder;

use crate::Args;
use crate::DeviceTrait;
use crate::Direction;
use crate::Driver;
use crate::Error;
use crate::Range;
use crate::RangeItem;

#[derive(Clone)]
pub struct AaroniaHttp {
    url: String,
}

pub struct RxStreamer {
    stream: Option<futures::stream::IntoStream<Body>>,
    buf: Bytes,
    items_left: usize,
}

pub struct TxStreamer {}

impl AaroniaHttp {
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let rt = Builder::new_current_thread().enable_all().build()?;

        rt.block_on(async {
            let url = args
                .get::<String>("url")
                .unwrap_or_else(|_| String::from("http://localhost:54664"));
            let url: Uri = url.parse().or(Err(Error::ValueError))?;

            let client = Client::new();
            let resp = client.get(url.clone()).await.or(Err(Error::Io))?;
            if resp.status().is_success() {
                Ok(vec![format!("driver=aarnia_http, url={url}").try_into()?])
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
            let a = v.remove(0);
            Ok(Self {
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
        todo!()
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

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        todo!()
    }

    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        todo!()
    }

    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        todo!()
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        timeout_us: i64,
    ) -> Result<usize, Error> {
        todo!()
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
