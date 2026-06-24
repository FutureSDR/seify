#![deny(missing_docs)]
//! Rust SDR hardware abstraction over multiple radio backends.
//!
//! Seify provides one API for probing, opening, configuring, and streaming from
//! SDR devices. Applications can use typed devices when the concrete backend is
//! known at compile time, or [`DynDevice`] when a driver is selected at runtime
//! through [`Args`].
//!
//! # Driver features
//!
//! The default feature set enables the `soapy` backend. Other backends are
//! enabled with Cargo features such as `rtlsdr`, `hackrfone`, `hydrasdr`,
//! `bladerf1`, `aaronia_http`, and `dummy`.
//!
//! Native Rust drivers are still experimental. For production use and the
//! widest set of stable hardware integrations, prefer the SoapySDR backend.
//!
//! # Example
//!
//! ```no_run
//! use num_complex::Complex32;
//! use seify::{DynDevice, RxStreamer};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let dev = DynDevice::from_args("driver=soapy")?;
//! let rx0 = dev.rx(0)?;
//! let mut rx = rx0.streamer()?;
//! let mut samples = [Complex32::new(0.0, 0.0); 1024];
//!
//! rx.activate()?;
//! let n = rx.read(&mut [&mut samples], 200_000)?;
//! println!("read {n} samples");
//! # Ok(())
//! # }
//! ```

mod args;
pub use args::Args;

mod async_compat;
mod async_device;
mod device;
pub use async_compat::AsyncBoxFuture;
pub use async_compat::AsyncFutureExt;
pub use async_compat::MaybeSend;
pub use async_compat::MaybeSync;
pub use async_device::AsyncAgc;
pub use async_device::AsyncAgcControl;
pub use async_device::AsyncAntenna;
pub use async_device::AsyncAntennaControl;
pub use async_device::AsyncBandwidth;
pub use async_device::AsyncBandwidthControl;
pub use async_device::AsyncChannelInfo;
pub use async_device::AsyncDcOffset;
pub use async_device::AsyncDcOffsetControl;
pub use async_device::AsyncDevice;
pub use async_device::AsyncDeviceInfo;
pub use async_device::AsyncDriverBackend;
pub use async_device::AsyncDynDevice;
pub use async_device::AsyncDynDeviceBackend;
pub use async_device::AsyncFrequency;
pub use async_device::AsyncFrequencyControl;
pub use async_device::AsyncGain;
pub use async_device::AsyncGainControl;
pub use async_device::AsyncRegistry;
pub use async_device::AsyncRxChannel;
pub use async_device::AsyncRxDevice;
pub use async_device::AsyncSampleRate;
pub use async_device::AsyncSampleRateControl;
pub use async_device::AsyncTxChannel;
pub use async_device::AsyncTxDevice;
pub use async_device::AsyncTypedDeviceBackend;
pub use async_device::DynAsyncRxStreamer;
pub use async_device::DynAsyncTxStreamer;
pub use async_device::ErasedAsyncRxDevice;
pub use async_device::ErasedAsyncTxDevice;
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

mod async_streamer;
mod streamer;
pub use async_streamer::AsyncRxStreamer;
pub use async_streamer::AsyncTxStreamer;
pub use streamer::RxStreamer;
pub use streamer::TxStreamer;

use serde::{Deserialize, Serialize};

use std::str::FromStr;
use thiserror::Error;

/// Device or channel capability used in semantic errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Capability {
    /// Channel count or channel metadata.
    ChannelInfo,
    /// Receive streaming.
    RxStreaming,
    /// Transmit streaming.
    TxStreaming,
    /// Antenna selection.
    Antenna,
    /// Automatic gain control.
    Agc,
    /// Manual gain control.
    Gain,
    /// Frequency tuning.
    Frequency,
    /// Sample-rate control.
    SampleRate,
    /// Bandwidth control.
    Bandwidth,
    /// DC offset correction.
    DcOffset,
    /// Device identifier lookup.
    DeviceId,
    /// Timed stream activation.
    TimedActivation,
    /// Timed stream deactivation.
    TimedDeactivation,
    /// Backend-specific driver operation.
    DriverOperation,
}

/// Driver-specific error details.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DriverError {
    #[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
    #[error("Soapy ({0})")]
    /// Error returned by the SoapySDR backend.
    Soapy(soapysdr::Error),
    #[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
    #[error("Ureq ({0})")]
    /// HTTP client error returned by the Aaronia HTTP backend.
    Ureq(Box<ureq::Error>),
    #[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
    #[error("RtlSdr ({0})")]
    /// Error returned by the RTL-SDR backend.
    RtlSdr(seify_rtlsdr::error::RtlsdrError),
    #[cfg(all(feature = "hackrfone", not(target_arch = "wasm32")))]
    #[error("Hackrf ({0})")]
    /// Error returned by the HackRF One backend.
    HackRfOne(seify_hackrfone::Error),
    #[cfg(all(feature = "hydrasdr", not(target_arch = "wasm32")))]
    #[error("HydraSdr ({0})")]
    /// Error returned by the HydraSDR backend.
    HydraSdr(hydrasdr_rs::Error),
    #[error("{0}")]
    /// Backend error represented as a string.
    Other(String),
}

/// Error returned by Seify operations.
#[derive(Debug, Error)]
pub enum Error {
    /// A device or channel does not expose the requested capability.
    #[error("unsupported capability {capability:?}")]
    Unsupported {
        /// Capability that is not supported.
        capability: Capability,
        /// Optional backend-specific reason.
        reason: Option<String>,
    },
    /// A channel index is invalid for the requested direction.
    #[error("invalid {direction:?} channel {channel}; available channels: {available}")]
    InvalidChannel {
        /// RX or TX direction.
        direction: Direction,
        /// Requested channel index.
        channel: usize,
        /// Number of channels available in the direction.
        available: usize,
    },
    /// An argument exists but has an invalid value.
    #[error("invalid argument {name}: {reason}")]
    InvalidArgument {
        /// Argument name.
        name: String,
        /// Reason the value is invalid.
        reason: String,
    },
    /// A required argument is missing.
    #[error("missing argument {name}")]
    MissingArgument {
        /// Missing argument name.
        name: String,
    },
    /// No matching device was found.
    #[error("device not found")]
    DeviceNotFound,
    /// A requested driver is not enabled in this build.
    #[error("driver feature not enabled for {driver:?}")]
    DriverFeatureNotEnabled {
        /// Driver whose Cargo feature is disabled.
        driver: Driver,
    },
    /// The requested driver does not match the typed backend.
    #[error("driver mismatch: expected {expected:?}, requested {requested:?}")]
    DriverMismatch {
        /// Driver implemented by the typed backend.
        expected: Driver,
        /// Driver requested in arguments.
        requested: Driver,
    },
    /// A numeric value is outside the supported range.
    #[error("value {value} for {name} out of range ({range:?})")]
    OutOfRange {
        /// Name of the setting being changed.
        name: String,
        /// Supported range.
        range: Range,
        /// Requested value.
        value: f64,
    },
    /// Device or stream resource is busy.
    #[error("busy")]
    Busy,
    /// Device was disconnected while in use.
    #[error("device disconnected")]
    DeviceDisconnected,
    /// Operation timed out.
    #[error("timeout")]
    Timeout,
    /// Stream operation requires an active stream.
    #[error("stream inactive")]
    StreamInactive,
    /// Stream has been closed.
    #[error("stream closed")]
    StreamClosed,
    /// RX stream overrun.
    #[error("overrun")]
    Overrun,
    /// TX stream underrun.
    #[error("underrun")]
    Underrun,
    /// JSON serialization or deserialization failed.
    #[error("Json ({0})")]
    Json(#[from] serde_json::Error),
    /// I/O operation failed.
    #[error("Io ({0})")]
    Io(#[from] std::io::Error),
    /// Backend driver error.
    #[error("driver error ({0})")]
    Driver(#[from] DriverError),
}

impl Error {
    /// Create an unsupported-capability error without a backend-specific reason.
    pub fn unsupported(capability: Capability) -> Self {
        Self::Unsupported {
            capability,
            reason: None,
        }
    }

    /// Create an unsupported-capability error with a backend-specific reason.
    pub fn unsupported_reason(capability: Capability, reason: impl Into<String>) -> Self {
        Self::Unsupported {
            capability,
            reason: Some(reason.into()),
        }
    }

    /// Create an invalid-argument error.
    pub fn invalid_argument(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidArgument {
            name: name.into(),
            reason: reason.into(),
        }
    }

    /// Create a missing-argument error.
    pub fn missing_argument(name: impl Into<String>) -> Self {
        Self::MissingArgument { name: name.into() }
    }

    /// Create an invalid-channel error.
    pub fn invalid_channel(direction: Direction, channel: usize, available: usize) -> Self {
        Self::InvalidChannel {
            direction,
            channel,
            available,
        }
    }

    /// Create an out-of-range error.
    pub fn out_of_range(name: impl Into<String>, range: Range, value: f64) -> Self {
        Self::OutOfRange {
            name: name.into(),
            range,
            value,
        }
    }

    /// Return `true` if this error reports an unsupported capability.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported { .. })
    }

    /// Return `true` if this error reports that no device was found.
    pub fn is_device_not_found(&self) -> bool {
        matches!(self, Self::DeviceNotFound)
    }

    /// Return `true` if this error reports a missing argument.
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
    /// Aaronia Spectran HTTP backend.
    AaroniaHttp,
    /// bladeRF 1 backend.
    BladeRf,
    /// Dummy for unit tests.
    Dummy,
    /// HackRF One backend.
    HackRf,
    /// HydraSDR backend.
    HydraSdr,
    /// RTL-SDR backend.
    RtlSdr,
    /// SoapySDR backend.
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

/// Signal direction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Direction {
    /// Receive direction.
    Rx,
    /// Transmit direction.
    Tx,
}

/// Enumerate devices.
///
/// ## Returns
///
/// A vector of [`Args`] that provide information about the device and can be used to identify it
/// uniquely, i.e., passing the [`Args`] to [`DynDevice::from_args`](crate::DynDevice::from_args) will
/// open this particular device.
pub fn enumerate() -> Result<Vec<Args>, Error> {
    enumerate_with_args(Args::new())
}

/// Enumerate devices with given [`Args`].
///
/// ## Returns
///
/// A vector of [`Args`] that provide information about the device and can be used to identify it
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
