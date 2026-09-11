//! Hardware drivers, implementing Seify's capability traits.

#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
pub mod aaronia_http;
#[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
pub use aaronia_http::AaroniaHttp;

/// bladeRF 1 backend.
#[cfg(feature = "bladerf1")]
pub mod bladerf1;
#[cfg(all(
    feature = "bladerf1",
    any(target_arch = "wasm32", feature = "smol", feature = "tokio")
))]
pub use bladerf1::AsyncBladeRf;
#[cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]
pub use bladerf1::BladeRf;

#[cfg(feature = "dummy")]
pub mod dummy;
#[cfg(feature = "dummy")]
pub use dummy::Dummy;

#[cfg(feature = "rtlsdr")]
pub mod rtlsdr;
#[cfg(all(
    feature = "rtlsdr",
    any(target_arch = "wasm32", feature = "smol", feature = "tokio")
))]
pub use rtlsdr::AsyncRtlSdr;
#[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
pub use rtlsdr::RtlSdr;

#[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
pub mod soapy;
#[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
pub use soapy::Soapy;

/// HackRF backend.
#[cfg(feature = "hackrf")]
pub mod hackrf;
#[cfg(all(
    feature = "hackrf",
    any(target_arch = "wasm32", feature = "smol", feature = "tokio")
))]
pub use hackrf::AsyncHackRf;
#[cfg(all(feature = "hackrf", not(target_arch = "wasm32")))]
pub use hackrf::HackRf;

#[cfg(feature = "hydrasdr")]
pub mod hydrasdr;
#[cfg(all(
    feature = "hydrasdr",
    any(target_arch = "wasm32", feature = "smol", feature = "tokio")
))]
pub use hydrasdr::AsyncHydraSdr;
#[cfg(all(feature = "hydrasdr", not(target_arch = "wasm32")))]
pub use hydrasdr::HydraSdr;
