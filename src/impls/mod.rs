//! Hardware drivers, implementing the [`DeviceTrait`](crate::DeviceTrait).

#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
pub mod aaronia_http;
#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
pub use aaronia_http::AaroniaHttp;

#[cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]
pub mod bladerf1;
#[cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]
pub use bladerf1::BladeRf;

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
