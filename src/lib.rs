mod args;
pub use args::Args;

mod device;
pub use device::Device;
pub use device::DeviceTrait;
pub use device::GenericDevice;

pub mod impls;

mod streamer;
pub use streamer::RxStreamer;
pub use streamer::TxStreamer;

use std::str::FromStr;
use thiserror::Error;

/// Seify Error
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("DeviceError")]
    DeviceError,
    #[error("Value Error")]
    ValueError,
    #[error("Not Found")]
    NotFound,
    #[error("Not Supported")]
    NotSupported,
    #[error("Inactive")]
    Inactive,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Driver {
    #[cfg(feature = "aaronia")]
    Aaronia,
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
        #[cfg(feature = "aaronia")]
        {
            if s == "aaronia" {
                return Ok(Driver::Aaronia);
            }
        }
        #[cfg(feature = "rtlsdr")]
        {
            if s == "rtlsdr" || s == "rtl-sdr" || s == "rtl" {
                return Ok(Driver::RtlSdr);
            }
        }
        #[cfg(feature = "hackrf")]
        {
            if s == "hackrf" {
                return Ok(Driver::HackRf);
            }
        }
        Err(Error::ValueError)
    }
}

#[derive(Debug, Clone, Copy)]
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

    #[cfg(feature = "aaronia")]
    {
        if driver.is_none() || driver.as_ref().unwrap() == "aaronia" {
            devs.append(&mut impls::Aaronia::probe(&args)?)
        }
    }

    #[cfg(feature = "rtlsdr")]
    {
        if driver.is_none() || driver.as_ref().unwrap() == "rtlsdr" {
            devs.append(&mut impls::RtlSdr::probe(&args)?)
        }
    }

    #[cfg(feature = "hackrf")]
    {
        if driver.is_none() || driver.as_ref().unwrap() == "hackrf" {
            devs.append(&mut impls::HackRf::probe(&args)?)
        }
    }

    Ok(devs)
}

/// Component of a [Range].
///
/// Can be an interval or an individual value.
#[derive(Debug, Clone)]
pub enum RangeItem {
    /// Interval (inclusive).
    Interval(f64, f64),
    /// Exact value.
    Value(f64),
}

/// Range of possible values, comprised of [RangeItem]s, which can be individual values or
/// Intervals.
#[derive(Debug, Clone)]
pub struct Range {
    items: Vec<RangeItem>,
}

impl Range {
    pub fn new(items: Vec<RangeItem>) -> Self {
        Self { items }
    }
    pub fn contains(&self, value: f64) -> bool {
        for item in &self.items {
            match *item {
                RangeItem::Interval(a, b) => {
                    if a <= value && value <= b {
                        return true;
                    }
                }
                RangeItem::Value(v) => {
                    if (v - value).abs() <= f64::EPSILON {
                        return true;
                    }
                }
            }
        }
        false
    }
}
