//! Hardware drivers, implementing the [`DeviceTrait`](crate::DeviceTrait).
#[cfg(all(feature = "aaronia", any(target_os = "linux", target_os = "windows")))]
pub mod aaronia;
#[cfg(all(feature = "aaronia", any(target_os = "linux", target_os = "windows")))]
pub use aaronia::Aaronia;

#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
pub mod aaronia_http;
#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
pub use aaronia_http::AaroniaHttp;

#[cfg(feature = "dummy")]
pub mod dummy;
#[cfg(feature = "dummy")]
pub use dummy::Dummy;

#[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
pub mod rtlsdr;
#[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
pub use rtlsdr::RtlSdr;

#[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
pub mod soapy;
#[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
pub use soapy::Soapy;

#[cfg(all(feature = "hackrfone", not(target_arch = "wasm32")))]
pub mod hackrfone;
#[cfg(all(feature = "hackrfone", not(target_arch = "wasm32")))]
pub use hackrfone::HackRfOne;
