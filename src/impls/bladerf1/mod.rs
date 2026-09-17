//! bladeRF 1 driver.
//!
//! libbladerf-rs mirrors nusb's runtime integration, so on native targets
//! the backend needs exactly one of Seify's `smol` or `tokio` features for
//! both its synchronous and asynchronous halves; libbladerf-rs reports a
//! compile error otherwise.

mod common;

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
mod asynchronous;
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
pub use asynchronous::{AsyncBladeRf, AsyncBladeRfRxStreamer, AsyncBladeRfTxStreamer};

#[cfg(not(target_arch = "wasm32"))]
mod sync;
#[cfg(not(target_arch = "wasm32"))]
pub use sync::{BladeRf, RxStreamer, TxStreamer};
