mod args;
pub use args::Args;

mod device;
pub use device::Agc;
pub use device::AgcControl;
pub use device::Antenna;
pub use device::AntennaControl;
pub use device::Bandwidth;
pub use device::BandwidthControl;
pub use device::ChannelCapabilities;
pub use device::ChannelControls;
pub use device::ChannelInfo;
pub use device::DcOffset;
pub use device::DcOffsetControl;
pub use device::Device;
pub use device::DeviceCapabilities;
pub use device::DeviceInfo;
pub use device::DynDevice;
pub use device::DynDeviceBackend;
pub use device::DynRxStreamer;
pub use device::DynTxStreamer;
pub use device::ErasedRxDevice;
pub use device::ErasedTxDevice;
pub use device::Frequency;
pub use device::FrequencyComponent;
pub use device::FrequencyControl;
pub use device::Gain;
pub use device::GainControl;
pub use device::GainElement;
pub use device::RxChannel;
pub use device::RxDevice;
pub use device::SampleRate;
pub use device::SampleRateControl;
pub use device::TxChannel;
pub use device::TxDevice;

pub mod impls;

mod registry;
pub use registry::DeviceDescriptor;
pub use registry::DriverBackend;
pub use registry::Registry;

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
    #[cfg(all(feature = "hydrasdr", not(target_arch = "wasm32")))]
    #[error("HydraSdr ({0})")]
    HydraSdr(#[from] hydrasdr_rs::Error),
}

#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
impl From<ureq::Error> for Error {
    fn from(value: ureq::Error) -> Self {
        Error::Ureq(Box::new(value))
    }
}

/// Supported hardware drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Driver {
    AaroniaHttp,
    BladeRf,
    Dummy,
    HackRf,
    HydraSdr,
    RtlSdr,
    Soapy,
}

impl FromStr for Driver {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
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
        if s == "hydrasdr" || s == "hydra-sdr" || s == "hydra" {
            return Ok(Driver::HydraSdr);
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
    Ok(Registry::default()
        .probe(a)?
        .into_iter()
        .map(DeviceDescriptor::into_args)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hydrasdr_driver_aliases_parse() {
        for alias in ["hydrasdr", "hydra-sdr", "hydra"] {
            assert_eq!(alias.parse::<Driver>().unwrap(), Driver::HydraSdr);
        }
    }

    #[test]
    fn native_aaronia_driver_is_not_parseable() {
        assert!(matches!(
            "aaronia".parse::<Driver>(),
            Err(Error::ValueError)
        ));
    }

    #[test]
    fn aaronia_http_driver_aliases_parse() {
        for alias in ["aaronia_http", "aaronia-http", "aaroniahttp"] {
            assert_eq!(alias.parse::<Driver>().unwrap(), Driver::AaroniaHttp);
        }
    }

    #[test]
    #[cfg(not(all(feature = "hydrasdr", not(target_arch = "wasm32"))))]
    fn hydrasdr_enumeration_reports_disabled_feature_when_not_enabled() {
        assert!(matches!(
            enumerate_with_args("driver=hydrasdr"),
            Err(Error::FeatureNotEnabled)
        ));
    }

    #[test]
    #[cfg(not(all(feature = "hydrasdr", not(target_arch = "wasm32"))))]
    fn hydrasdr_from_args_reports_disabled_feature_when_not_enabled() {
        assert!(matches!(
            Device::from_args("driver=hydrasdr"),
            Err(Error::FeatureNotEnabled)
        ));
    }
}
