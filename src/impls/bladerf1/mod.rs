//! bladeRF 1 driver.
//!
//! On native targets the synchronous backend (`BladeRf`) needs no async
//! runtime: it drives every `libbladerf-rs` operation through `.wait()`, which
//! runs nusb's blocking syscalls inline on the calling thread. The
//! asynchronous backend (`AsyncBladeRf`) additionally needs exactly one of
//! Seify's `smol` or `tokio` features, because awaiting `libbladerf-rs`
//! operations requires nusb's runtime integration. On `wasm32` only the
//! asynchronous backend is compiled.

mod common;
mod convert;
mod selector;

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
mod asynchronous;
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
pub use asynchronous::{AsyncBladeRf, AsyncBladeRfRxStreamer, AsyncBladeRfTxStreamer};

#[cfg(not(target_arch = "wasm32"))]
mod sync;
#[cfg(not(target_arch = "wasm32"))]
pub use sync::{BladeRf, RxStreamer, TxStreamer};
