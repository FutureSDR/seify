//! PlutoSDR RX streaming and AD936x configuration over native IIO USB.
//!
//! One RX channel supports frequency, rate, bandwidth, gain, AGC, DC tracking, and port
//! selection. Streams own a separate USB pipe and expose normalized Complex32.
//! RF and baseband DC tracking are enabled on open when supported. Setting
//! sample rate also matches analog bandwidth; set bandwidth afterwards to override.
//! TX and timed activation are not implemented.

mod common;

#[cfg(not(target_arch = "wasm32"))]
mod sync;
#[cfg(not(target_arch = "wasm32"))]
pub use sync::{Pluto, PlutoRxStreamer};

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
mod asynchronous;
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
pub use asynchronous::{AsyncPluto, AsyncPlutoRxStreamer};

pub use plutosdr::iiod::Context as IioContext;
