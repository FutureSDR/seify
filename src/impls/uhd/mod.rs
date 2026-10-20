//! Native Rust USRP B2xx receive backend using `uhd-rs`.
//!
//! Exposes channel zero with RX2 / TX/RX connector selection, analog bandwidth
//! and automatic DC offset correction controls. Firmware and FPGA images are embedded by the
//! driver. Supports B200, B210, B200mini, and B205mini; TX and multichannel
//! streaming are not yet exposed.

mod common;
#[cfg(not(target_arch = "wasm32"))]
mod sync;
#[cfg(not(target_arch = "wasm32"))]
pub use sync::{RxStreamer, Uhd};
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
mod asynchronous;
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
pub use asynchronous::{AsyncUhd, AsyncUhdRxStreamer};
