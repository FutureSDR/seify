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
    #[error("Io")]
    Io,
}

impl From<std::io::Error> for Error {
    fn from(_value: std::io::Error) -> Self {
        Error::Io
    }
}

#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum Driver {
    #[cfg(feature = "aaronia")]
    Aaronia,
    #[cfg(feature = "aaronia_http")]
    AaroniaHttp,
    #[cfg(feature = "hackrf")]
    HackRf,
    #[cfg(feature = "rtlsdr")]
    RtlSdr,
    #[cfg(feature = "soapy")]
    Soapy,
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
        #[cfg(feature = "aaronia_http")]
        {
            if s == "aaronia_http" || s == "aaronia-http" || s == "aaroniahttp" {
                return Ok(Driver::AaroniaHttp);
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
        #[cfg(feature = "soapy")]
        {
            if s == "soapy" || s == "soapysdr" {
                return Ok(Driver::Soapy);
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
    let driver = match args.get::<String>("driver") {
        Ok(s) => Some(s.parse::<Driver>()?),
        Err(_) => None,
    };

    #[cfg(feature = "aaronia")]
    {
        if driver.is_none() || driver.as_ref().unwrap() == &Driver::Aaronia {
            devs.append(&mut impls::Aaronia::probe(&args)?)
        }
    }

    #[cfg(feature = "aaronia_http")]
    {
        if driver.is_none() || driver.as_ref().unwrap() == &Driver::AaroniaHttp {
            devs.append(&mut impls::AaroniaHttp::probe(&args)?)
        }
    }

    #[cfg(feature = "rtlsdr")]
    {
        if driver.is_none() || driver.as_ref().unwrap() == &Driver::RtlSdr {
            devs.append(&mut impls::RtlSdr::probe(&args)?)
        }
    }

    #[cfg(feature = "hackrf")]
    {
        if driver.is_none() || driver.as_ref().unwrap() == &Driver::HackRf {
            devs.append(&mut impls::HackRf::probe(&args)?)
        }
    }

    #[cfg(feature = "soapy")]
    {
        if driver.is_none() || driver.as_ref().unwrap() == &Driver::Soapy {
            devs.append(&mut impls::Soapy::probe(&args)?)
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
    pub fn closest(&self, value: f64) -> Option<f64> {
        fn closer(target: f64, closest: Option<f64>, current: f64) -> f64 {
            match closest {
                Some(c) => {
                    if (target - current).abs() < (c - target).abs() {
                        current
                    } else {
                        c
                    }
                }
                None => current,
            }
        }

        if self.contains(value) {
            Some(value)
        } else {
            let mut close = None;
            for i in self.items.iter() {
                match i {
                    RangeItem::Interval(a, b) => {
                        close = Some(closer(value, close, *a));
                        close = Some(closer(value, close, *b));
                    }
                    RangeItem::Value(a) => {
                        close = Some(closer(value, close, *a));
                    }
                }
            }
            close
        }
    }
    pub fn at_least(&self, value: f64) -> Option<f64> {
        fn closer_at_least(target: f64, closest: Option<f64>, current: f64) -> Option<f64> {
            match closest {
                Some(c) => {
                    if (target - current).abs() < (c - target).abs() && current >= target {
                        Some(current)
                    } else {
                        closest
                    }
                }
                None => {
                    if current >= target {
                        Some(current)
                    } else {
                        None
                    }
                }
            }
        }

        if self.contains(value) {
            Some(value)
        } else {
            let mut close = None;
            for i in self.items.iter() {
                match i {
                    RangeItem::Interval(a, b) => {
                        close = closer_at_least(value, close, *a);
                        close = closer_at_least(value, close, *b);
                    }
                    RangeItem::Value(a) => {
                        close = closer_at_least(value, close, *a);
                    }
                }
            }
            close
        }
    }
    pub fn at_max(&self, value: f64) -> Option<f64> {
        fn closer_at_max(target: f64, closest: Option<f64>, current: f64) -> Option<f64> {
            match closest {
                Some(c) => {
                    if (target - current).abs() < (c - target).abs() && current <= target {
                        Some(current)
                    } else {
                        closest
                    }
                }
                None => {
                    if current <= target {
                        Some(current)
                    } else {
                        None
                    }
                }
            }
        }

        if self.contains(value) {
            Some(value)
        } else {
            let mut close = None;
            for i in self.items.iter() {
                match i {
                    RangeItem::Interval(a, b) => {
                        close = closer_at_max(value, close, *a);
                        close = closer_at_max(value, close, *b);
                    }
                    RangeItem::Value(a) => {
                        close = closer_at_max(value, close, *a);
                    }
                }
            }
            close
        }
    }
    pub fn merge(&mut self, mut r: Range) {
        self.items.append(&mut r.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_empty() {
        let r = Range::new(Vec::new());
        assert!(!r.contains(123.0));
    }
    #[test]
    fn contains() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
        ]);
        assert!(r.contains(123.0));
        assert!(r.contains(23.0));
        assert!(r.contains(42.0));
        assert!(r.contains(40.0));
        assert!(!r.contains(19.0));
    }
    #[test]
    fn closest() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
        ]);
        assert_eq!(r.closest(100.0), Some(123.0));
        assert_eq!(r.closest(1000.0), Some(123.0));
        assert_eq!(r.closest(30.0), Some(30.0));
        assert_eq!(r.closest(20.0), Some(23.0));
        assert_eq!(r.closest(50.0), Some(42.0));
    }
    #[test]
    fn at_least() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
        ]);
        assert_eq!(r.at_least(100.0), Some(123.0));
        assert_eq!(r.at_least(1000.0), None);
        assert_eq!(r.at_least(30.0), Some(30.0));
        assert_eq!(r.at_least(10.0), Some(23.0));
    }
    #[test]
    fn at_max() {
        let r = Range::new(vec![
            RangeItem::Value(123.0),
            RangeItem::Interval(23.0, 42.0),
        ]);
        assert_eq!(r.at_max(100.0), Some(42.0));
        assert_eq!(r.at_max(10.0), None);
        assert_eq!(r.at_max(30.0), Some(30.0));
        assert_eq!(r.at_max(50.0), Some(42.0));
    }
}
