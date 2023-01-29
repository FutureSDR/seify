//! Hardware drivers, implementing the [`DeviceTrait`](crate::DeviceTrait).
#[cfg(feature = "aaronia")]
pub mod aaronia;
#[cfg(feature = "aaronia")]
pub use aaronia::Aaronia;

#[cfg(feature = "aaronia_http")]
pub mod aaronia_http;
#[cfg(feature = "aaronia_http")]
pub use aaronia_http::AaroniaHttp;

#[cfg(feature = "rtlsdr")]
pub mod rtlsdr;
#[cfg(feature = "rtlsdr")]
pub use rtlsdr::RtlSdr;

#[cfg(feature = "soapy")]
pub mod soapy;
#[cfg(feature = "soapy")]
pub use soapy::Soapy;
