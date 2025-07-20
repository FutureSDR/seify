mod args;
pub use args::Args;

mod device;
pub use device::Device;
pub use device::DeviceTrait;
pub use device::GenericDevice;

pub mod impls;

mod range;
pub use range::Range;
pub use range::RangeItem;

mod streamer;
pub use streamer::RxStreamer;
pub use streamer::TxStreamer;

use serde::{Deserialize, Serialize};

use std::str::FromStr;
use thiserror::Error;

/// Seify Error
#[derive(Debug, Error)]
pub enum Error {
    #[error("DeviceError")]
    DeviceError,
    #[error("Value ({1}) out of range ({0:?})")]
    OutOfRange(Range, f64),
    #[error("Value Error")]
    ValueError,
    #[error("Not Found")]
    NotFound,
    #[error("corresponding feature not enabled")]
    FeatureNotEnabled,
    #[error("Not Supported")]
    NotSupported,
    #[error("Overflow")]
    Overflow,
    #[error("Inactive")]
    Inactive,
    #[error("Json ({0})")]
    Json(#[from] serde_json::Error),
    #[error("Misc")]
    Misc(String),
    #[error("Io ({0})")]
    Io(#[from] std::io::Error),
    #[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
    #[error("Soapy ({0})")]
    Soapy(soapysdr::Error),
    #[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
    #[error("Ureq ({0})")]
    Ureq(Box<ureq::Error>),
    #[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
    #[error("RtlSdr ({0})")]
    RtlSdr(#[from] seify_rtlsdr::error::RtlsdrError),
    #[cfg(all(feature = "hackrfone", not(target_arch = "wasm32")))]
    #[error("Hackrf ({0})")]
    HackRfOne(#[from] seify_hackrfone::Error),
}

#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
impl From<ureq::Error> for Error {
    fn from(value: ureq::Error) -> Self {
        Error::Ureq(Box::new(value))
    }
}

/// Supported hardware drivers.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Driver {
    Aaronia,
    AaroniaHttp,
    BladeRf,
    Dummy,
    HackRf,
    RtlSdr,
    Soapy,
}

impl FromStr for Driver {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        if s == "aaronia" {
            return Ok(Driver::Aaronia);
        }
        if s == "aaronia_http" || s == "aaronia-http" || s == "aaroniahttp" {
            return Ok(Driver::AaroniaHttp);
        }
        if s == "bladerf" || s == "bladerf1" || s == "BladeRf" {
            return Ok(Driver::BladeRf);
        }
        if s == "rtlsdr" || s == "rtl-sdr" || s == "rtl" {
            return Ok(Driver::RtlSdr);
        }
        if s == "soapy" || s == "soapysdr" {
            return Ok(Driver::Soapy);
        }
        if s == "hackrf" || s == "hackrfone" {
            return Ok(Driver::HackRf);
        }
        if s == "dummy" || s == "Dummy" {
            return Ok(Driver::Dummy);
        }
        Err(Error::ValueError)
    }
}

/// Direction (Rx/TX)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Direction {
    Rx,
    Tx,
}

/// Enumerate devices.
///
/// ## Returns
///
/// A vector or [`Args`] that provide information about the device and can be used to identify it
/// uniquely, i.e., passing the [`Args`] to [`Device::from_args`](crate::Device::from_args) will
/// open this particular device.
pub fn enumerate() -> Result<Vec<Args>, Error> {
    enumerate_with_args(Args::new())
}

/// Enumerate devices with given [`Args`].
///
/// ## Returns
///
/// A vector or [`Args`] that provide information about the device and can be used to identify it
/// uniquely, i.e., passing the [`Args`] to [`Device::from_args`](crate::Device::from_args) will
/// open this particular device.
pub fn enumerate_with_args<A: TryInto<Args>>(a: A) -> Result<Vec<Args>, Error> {
    let args: Args = a.try_into().or(Err(Error::ValueError))?;
    let mut devs = Vec::new();
    let driver = match args.get::<String>("driver") {
        Ok(s) => Some(s.parse::<Driver>()?),
        Err(_) => None,
    };

    #[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
    {
        if driver.is_none() || matches!(driver, Some(Driver::AaroniaHttp)) {
            devs.append(&mut impls::AaroniaHttp::probe(&args)?)
        }
    }
    #[cfg(not(all(feature = "aaronia_http", not(target_arch = "wasm32"))))]
    {
        if matches!(driver, Some(Driver::AaroniaHttp)) {
            return Err(Error::FeatureNotEnabled);
        }
    }

    #[cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]
    {
        if driver.is_none() || matches!(driver, Some(Driver::BladeRf)) {
            devs.append(&mut impls::BladeRf::probe(&args)?)
        }
    }
    #[cfg(not(all(feature = "bladerf1", not(target_arch = "wasm32"))))]
    {
        if matches!(driver, Some(Driver::BladeRf)) {
            return Err(Error::FeatureNotEnabled);
        }
    }

    #[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
    {
        if driver.is_none() || matches!(driver, Some(Driver::RtlSdr)) {
            devs.append(&mut impls::RtlSdr::probe(&args)?)
        }
    }
    #[cfg(not(all(feature = "rtlsdr", not(target_arch = "wasm32"))))]
    {
        if matches!(driver, Some(Driver::RtlSdr)) {
            return Err(Error::FeatureNotEnabled);
        }
    }

    #[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
    {
        if driver.is_none() || matches!(driver, Some(Driver::Soapy)) {
            devs.append(&mut impls::Soapy::probe(&args)?)
        }
    }
    #[cfg(not(all(feature = "soapy", not(target_arch = "wasm32"))))]
    {
        if matches!(driver, Some(Driver::Soapy)) {
            return Err(Error::FeatureNotEnabled);
        }
    }

    #[cfg(all(feature = "hackrfone", not(target_arch = "wasm32")))]
    {
        if driver.is_none() || matches!(driver, Some(Driver::HackRf)) {
            devs.append(&mut impls::HackRfOne::probe(&args)?)
        }
    }
    #[cfg(not(all(feature = "hackrfone", not(target_arch = "wasm32"))))]
    {
        if matches!(driver, Some(Driver::HackRf)) {
            return Err(Error::FeatureNotEnabled);
        }
    }
    #[cfg(feature = "dummy")]
    {
        if driver.is_none() || matches!(driver, Some(Driver::Dummy)) {
            devs.append(&mut impls::Dummy::probe(&args)?)
        }
    }
    #[cfg(not(feature = "dummy"))]
    {
        if matches!(driver, Some(Driver::Dummy)) {
            return Err(Error::FeatureNotEnabled);
        }
    }

    let _ = &mut devs;
    Ok(devs)
}
