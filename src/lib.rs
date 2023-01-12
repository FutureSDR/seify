mod args;
pub use args::Args;

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
use thiserror::Error;

/// Seify Error
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("Value Error")]
    ValueError,
    #[error("Not Found")]
    NotFound,
}

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

pub fn enumerate() -> Result<Vec<Args>, Error> {
    enumerate_with_args("")
}
pub fn enumerate_with_args<A: AsRef<str>>(a: A) -> Result<Vec<Args>, Error> {
    let args: Args = a.as_ref().parse()?;
    let mut devs = Vec::new();
    let driver = args.get::<String>("driver").ok();

    if cfg!(feature = "rtlsdr") && (driver.is_none() || driver.as_ref().unwrap() == "rtlsdr") {
        devs.append(&mut RtlSdr::probe(&args)?)
    }
    if cfg!(feature = "hackrf") && (driver.is_none() || driver.as_ref().unwrap() == "rtlsdr") {
        devs.append(&mut HackRf::probe(&args)?)
    }

    Ok(Vec::new())
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
