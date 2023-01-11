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
        (&mut self.dev as &mut dyn Any).downcast_mut::<D>().ok_or(Error::ValueError)
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
#[derive(Debug, Error)]
pub enum Error {
    #[error("Value Error")]
    ValueError,
    #[error("Not Found")]
    NotFound,
}

/// Config Value
#[derive(Clone)]
pub struct Value {
    v: String,
}

impl Value {
    pub fn parse<E, T: FromStr<Err=E>>(&self) -> Result<T, Error> {
        self.v.parse().or(Err(Error::ValueError))
    }
}

/// Configuration.
pub struct Config {
    map: HashMap<String, Value>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
    pub fn get<S: AsRef<str>>(&self, v: S) -> Result<Value, Error> {
        self.map
            .get(v.as_ref())
            .ok_or(Error::NotFound)
            .cloned()
    }
    pub fn set<S: Into<String>>(&mut self, key: S, value: Value) -> Option<Value> {
        self.map
            .insert(key.into(), value)
    }
}

impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut c = Config::new();
        let mut s = s.to_string();

        Err(Error::ValueError)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
