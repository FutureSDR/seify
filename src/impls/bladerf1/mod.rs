//! bladeRF 1 driver.

mod common;

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
mod asynchronous;
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
pub use asynchronous::{AsyncBladeRf, AsyncBladeRfRxStreamer, AsyncBladeRfTxStreamer};

#[cfg(not(target_arch = "wasm32"))]
mod sync;
#[cfg(not(target_arch = "wasm32"))]
pub use sync::{BladeRf, RxStreamer, TxStreamer};
