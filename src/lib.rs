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

#[cfg(all(feature = "web", not(target_arch = "wasm32")))]
pub(crate) mod web;
#[cfg(all(feature = "web", not(target_arch = "wasm32")))]
pub use web::{Connect, DefaultConnector, DefaultExecutor, Executor};

// Reexports
#[cfg(all(feature = "web", not(target_arch = "wasm32")))]
pub use ::hyper;
#[cfg(all(feature = "web", not(target_arch = "wasm32")))]
pub use tokio;

use std::str::FromStr;
use thiserror::Error;

/// Seify Error
#[derive(Debug, Clone, Error, PartialEq, Serialize, Deserialize)]
pub enum Error {
    #[error("DeviceError")]
    DeviceError,
    #[error("value out of range")]
    OutOfRange,
    #[error("Value Error")]
    ValueError,
    #[error("Not Found")]
    NotFound,
    #[error("corresponding feature not enabled")]
    FeatureNotEnabled,
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

/// Supported hardware drivers.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Driver {
    Aaronia,
    AaroniaHttp,
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
        if s == "rtlsdr" || s == "rtl-sdr" || s == "rtl" {
            return Ok(Driver::RtlSdr);
        }
        if s == "soapy" || s == "soapysdr" {
            return Ok(Driver::Soapy);
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

    #[cfg(all(feature = "aaronia", any(target_os = "linux", target_os = "windows")))]
    {
        if driver.is_none() || matches!(driver, Some(Driver::Aaronia)) {
            devs.append(&mut impls::Aaronia::probe(&args)?)
        }
    }
    #[cfg(not(all(feature = "aaronia", any(target_os = "linux", target_os = "windows"))))]
    {
        if matches!(driver, Some(Driver::Aaronia)) {
            return Err(Error::FeatureNotEnabled);
        }
    }

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

    let _ = &mut devs;
    Ok(devs)
}
