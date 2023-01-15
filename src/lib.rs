mod args;
pub use args::Args;

mod device;
pub use device::Device;
pub use device::DeviceTrait;
use std::str::FromStr;

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

use thiserror::Error;

/// Seify Error
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("Value Error")]
    ValueError,
    #[error("Not Found")]
    NotFound,
}

#[derive(Debug)]
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

impl FromStr for Driver {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        if cfg!(feature = "rtlsdr") && (s == "rtlsdr" || s == "rtl-sdr" || s == "rtl") {
            return Ok(Driver::RtlSdr);
        }
        if cfg!(feature = "hackrf") && (s == "hackrf") {
            return Ok(Driver::HackRf);
        }
        Err(Error::ValueError)
    }
}

pub enum Direction {
    Rx,
    Tx,
}

/// Enumerate devices.
pub fn enumerate() -> Result<Vec<Args>, Error> {
    enumerate_with_args(Args::new())
}

/// Enumerate devices with given [Args].
pub fn enumerate_with_args<A: TryInto<Args>>(a: A) -> Result<Vec<Args>, Error> {
    let args: Args = a.try_into().or(Err(Error::ValueError))?;
    let mut devs = Vec::new();
    let driver = args.get::<String>("driver").ok();

    if cfg!(feature = "rtlsdr") && (driver.is_none() || driver.as_ref().unwrap() == "rtlsdr") {
        devs.append(&mut RtlSdr::probe(&args)?)
    }
    if cfg!(feature = "hackrf") && (driver.is_none() || driver.as_ref().unwrap() == "hackrf") {
        devs.append(&mut HackRf::probe(&args)?)
    }

    Ok(Vec::new())
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
