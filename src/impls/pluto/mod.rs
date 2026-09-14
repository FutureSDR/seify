//! PlutoSDR discovery and IIO context inspection over native USB.
//!
//! The underlying driver currently implements PRINT and pipe lifecycle only.
//! This backend exposes no Seify RX/TX channels or RF control capabilities.
//! Typed backends provide the full context snapshot and explicit shutdown.

mod common;

#[cfg(not(target_arch = "wasm32"))]
mod sync;
#[cfg(not(target_arch = "wasm32"))]
pub use sync::Pluto;

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
mod asynchronous;
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
pub use asynchronous::AsyncPluto;

pub use plutosdr::iiod::Context as IioContext;
