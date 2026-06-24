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
pub use registry::TypedDeviceBackend;

mod range;
pub use range::Range;
pub use range::RangeItem;

mod streamer;
pub use streamer::RxStreamer;
pub use streamer::TxStreamer;

use serde::{Deserialize, Serialize};

use std::str::FromStr;
use thiserror::Error;

/// Device or channel capability used in semantic errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Capability {
    ChannelInfo,
    RxStreaming,
    TxStreaming,
    Antenna,
    Agc,
    Gain,
    Frequency,
    SampleRate,
    Bandwidth,
    DcOffset,
    DeviceId,
    TimedActivation,
    TimedDeactivation,
    DriverOperation,
}

/// Driver-specific error details.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DriverError {
    #[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
    #[error("Soapy ({0})")]
    Soapy(soapysdr::Error),
    #[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
    #[error("Ureq ({0})")]
    Ureq(Box<ureq::Error>),
    #[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
    #[error("RtlSdr ({0})")]
    RtlSdr(seify_rtlsdr::error::RtlsdrError),
    #[cfg(all(feature = "hackrfone", not(target_arch = "wasm32")))]
    #[error("Hackrf ({0})")]
    HackRfOne(seify_hackrfone::Error),
    #[cfg(all(feature = "hydrasdr", not(target_arch = "wasm32")))]
    #[error("HydraSdr ({0})")]
    HydraSdr(hydrasdr_rs::Error),
    #[error("{0}")]
    Other(String),
}

/// Seify Error
#[derive(Debug, Error)]
pub enum Error {
    #[error("unsupported capability {capability:?}")]
    Unsupported {
        capability: Capability,
        reason: Option<String>,
    },
    #[error("invalid {direction:?} channel {channel}; available channels: {available}")]
    InvalidChannel {
        direction: Direction,
        channel: usize,
        available: usize,
    },
    #[error("invalid argument {name}: {reason}")]
    InvalidArgument { name: String, reason: String },
    #[error("missing argument {name}")]
    MissingArgument { name: String },
    #[error("device not found")]
    DeviceNotFound,
    #[error("driver feature not enabled for {driver:?}")]
    DriverFeatureNotEnabled { driver: Driver },
    #[error("driver mismatch: expected {expected:?}, requested {requested:?}")]
    DriverMismatch { expected: Driver, requested: Driver },
    #[error("value {value} for {name} out of range ({range:?})")]
    OutOfRange {
        name: String,
        range: Range,
        value: f64,
    },
    #[error("busy")]
    Busy,
    #[error("device disconnected")]
    DeviceDisconnected,
    #[error("timeout")]
    Timeout,
    #[error("stream inactive")]
    StreamInactive,
    #[error("stream closed")]
    StreamClosed,
    #[error("overrun")]
    Overrun,
    #[error("underrun")]
    Underrun,
    #[error("Json ({0})")]
    Json(#[from] serde_json::Error),
    #[error("Io ({0})")]
    Io(#[from] std::io::Error),
    #[error("driver error ({0})")]
    Driver(#[from] DriverError),
}

impl Error {
    pub fn unsupported(capability: Capability) -> Self {
        Self::Unsupported {
            capability,
            reason: None,
        }
    }

    pub fn unsupported_reason(capability: Capability, reason: impl Into<String>) -> Self {
        Self::Unsupported {
            capability,
            reason: Some(reason.into()),
        }
    }

    pub fn invalid_argument(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidArgument {
            name: name.into(),
            reason: reason.into(),
        }
    }

    pub fn missing_argument(name: impl Into<String>) -> Self {
        Self::MissingArgument { name: name.into() }
    }

    pub fn invalid_channel(direction: Direction, channel: usize, available: usize) -> Self {
        Self::InvalidChannel {
            direction,
            channel,
            available,
        }
    }

    pub fn out_of_range(name: impl Into<String>, range: Range, value: f64) -> Self {
        Self::OutOfRange {
            name: name.into(),
            range,
            value,
        }
    }

    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported { .. })
    }

    pub fn is_device_not_found(&self) -> bool {
        matches!(self, Self::DeviceNotFound)
    }

    pub fn is_missing_argument(&self) -> bool {
        matches!(self, Self::MissingArgument { .. })
    }
}

#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
impl From<ureq::Error> for Error {
    fn from(value: ureq::Error) -> Self {
        Error::Driver(DriverError::Ureq(Box::new(value)))
    }
}

#[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
impl From<seify_rtlsdr::error::RtlsdrError> for Error {
    fn from(value: seify_rtlsdr::error::RtlsdrError) -> Self {
        Error::Driver(DriverError::RtlSdr(value))
    }
}

#[cfg(all(feature = "hackrfone", not(target_arch = "wasm32")))]
impl From<seify_hackrfone::Error> for Error {
    fn from(value: seify_hackrfone::Error) -> Self {
        Error::Driver(DriverError::HackRfOne(value))
    }
}

#[cfg(all(feature = "hydrasdr", not(target_arch = "wasm32")))]
impl From<hydrasdr_rs::Error> for Error {
    fn from(value: hydrasdr_rs::Error) -> Self {
        Error::Driver(DriverError::HydraSdr(value))
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
        Err(Error::invalid_argument("driver", "unknown driver"))
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
/// uniquely, i.e., passing the [`Args`] to [`DynDevice::from_args`](crate::DynDevice::from_args) will
/// open this particular device.
pub fn enumerate() -> Result<Vec<Args>, Error> {
    enumerate_with_args(Args::new())
}

/// Enumerate devices with given [`Args`].
///
/// ## Returns
///
/// A vector or [`Args`] that provide information about the device and can be used to identify it
/// uniquely, i.e., passing the [`Args`] to [`DynDevice::from_args`](crate::DynDevice::from_args) will
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
            Err(Error::InvalidArgument { name, .. }) if name == "driver"
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
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::HydraSdr
            })
        ));
    }

    #[test]
    #[cfg(not(all(feature = "hydrasdr", not(target_arch = "wasm32"))))]
    fn hydrasdr_from_args_reports_disabled_feature_when_not_enabled() {
        let result = DynDevice::from_args("driver=hydrasdr");
        assert!(matches!(
            result,
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::HydraSdr
            })
        ));
    }
}
