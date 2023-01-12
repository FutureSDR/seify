// #[cfg(feature = "aaronia")]
// pub mod aaronia;
// #[cfg(feature = "aaronia")]
// pub use aaronia::Http;

#[cfg(feature = "hackrf")]
pub mod hackrf;
#[cfg(feature = "hackrf")]
pub use hackrf::HackRf;

#[cfg(feature = "rtlsdr")]
pub mod rtlsdr;
#[cfg(feature = "rtlsdr")]
pub use rtlsdr::RtlSdr;

// #[cfg(feature = "soapy")]
// pub mod soapy;
// #[cfg(feature = "soapy")]
// pub use soapy::Soapy;

use std::any::Any;
use std::collections::HashMap;
use std::str::FromStr;
use thiserror::Error;

pub enum Driver {
    // #[cfg(feature = "aaronia")]
    // AaroniaHttp,
    #[cfg(feature = "hackrf")]
    HackRf,
    #[cfg(feature = "rtlsdr")]
    RtlSdr,
    // #[cfg(feature = "soapy")]
    // Soapy,
}

pub trait DeviceTrait {
    fn driver(&self) -> Driver;
    fn serial(&self) -> Option<String>;
    fn url(&self) -> Option<String>;
}
pub struct Device<T: DeviceTrait + Any> {
    dev: T,
}

impl<T: DeviceTrait + Any> Device<T> {
    pub fn inner<D: Any>(&mut self) -> Result<&mut D, Error> {
        (&mut self.dev as &mut dyn Any)
            .downcast_mut::<D>()
            .ok_or(Error::ValueError)
    }
    pub fn driver(&self) -> Driver {
        self.dev.driver()
    }
    pub fn serial(&self) -> Option<String> {
        self.dev.serial()
    }
    pub fn url(&self) -> Option<String> {
        self.dev.url()
    }
}

pub struct RXStreamer {}

pub struct TXStreamer {}

/// Frequency Range or item.
#[derive(Debug)]
pub enum Frequency {
    /// Range inclusive.
    Range(f64, f64),
    /// Exact frequency.
    Val(f64),
}

pub struct SupportedFrequencies {
    freqs: Vec<Frequency>,
}

impl SupportedFrequencies {
    pub fn new(freqs: Vec<Frequency>) -> Self {
        Self { freqs }
    }
    pub fn contains(&self, freq: f64) -> bool {
        for item in &self.freqs {
            match *item {
                Frequency::Range(a, b) => {
                    if a <= freq && freq <= b {
                        return true;
                    }
                }
                Frequency::Val(v) => {
                    if (v - freq).abs() <= f64::EPSILON {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Seify Error
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("Value Error")]
    ValueError,
    #[error("Not Found")]
    NotFound,
}

/// Configuration.
#[derive(Debug)]
pub struct Config {
    map: HashMap<String, String>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
    pub fn get<V: FromStr<Err = impl std::error::Error>>(
        &self,
        v: impl AsRef<str>,
    ) -> Result<V, Error> {
        self.map
            .get(v.as_ref())
            .ok_or(Error::NotFound)
            .and_then(|v| v.parse().or(Err(Error::ValueError)))
    }
    pub fn set<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) -> Option<String> {
        self.map.insert(key.into(), value.into())
    }
}

use nom::bytes::complete::tag;
use nom::multi::separated_list0;
use nom::sequence::separated_pair;
use nom::error::{FromExternalError, ParseError};
use nom::IResult;
use nom::character::complete::alphanumeric1;

fn parse_string<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + FromExternalError<&'a str, std::num::ParseIntError> + std::fmt::Debug,
{
    alphanumeric1(input)
}

impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = separated_list0(
            tag(","),
            separated_pair(parse_string::<nom::error::Error<_>>, tag("="), parse_string),
        )(s)
        .or(Err(Error::ValueError))?;
        Ok(Config {
            map: HashMap::from_iter(v.1.iter().cloned().map(|(a, b)| (a.into(), b.into()))),
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deserialize_empty() {
        let c: Config = "".parse().unwrap();
        assert_eq!(c.map.len(), 0);
    }
    #[test]
    fn deserialize_single() {
        let c: Config = "foo=bar".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.map.len(), 1);
    }
    #[test]
    fn deserialize_more() {
        let c: Config = "foo=bar,fo=ba".parse().unwrap();
        assert_eq!(c.get::<String>("foo").unwrap(), "bar");
        assert_eq!(c.get::<String>("fo").unwrap(), "ba");
        assert_eq!(c.map.len(), 2);
    }
    // #[test]
    // fn deserialize_quoted() {
    //     let c: Config = "foo=bar,fo=\"ba ,\"".parse().unwrap();
    //     assert_eq!(c.get::<String>("foo").unwrap(), "bar");
    //     assert_eq!(c.get::<String>("fo").unwrap(), "ba ,");
    //     assert_eq!(c.map.len(), 2);
    // }
    #[test]
    fn config_get() {
        let c: Config = "foo=123,bar=lol".parse().unwrap();
        assert_eq!(c.map.len(), 2);
        assert_eq!(c.get::<u32>("foo").unwrap(), 123);
        assert_eq!(c.get::<String>("foo").unwrap(), "123");
        assert_eq!(c.get::<String>("fooo"), Err(Error::NotFound));
        assert_eq!(c.get::<String>("bar").unwrap(), "lol");
        assert_eq!(c.get::<u32>("bar"), Err(Error::ValueError));
    }
}
