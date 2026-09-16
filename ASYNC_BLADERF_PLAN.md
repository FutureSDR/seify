# Async bladeRF 1 backend plan

Status: implemented on branch `async-bladerf` (steps 1–4 of §3 verified on hardware; §3.5/§3.6 open). Depends on libbladerf-rs ≥ 0.5 (branch
`async-interface` in `../libbladerf-rs`, plan in
`../libbladerf-rs/ASYNC_PLAN.md`).

Goal: give seify a bladeRF 1 backend in both worlds — the sync `Device` API
(native) and the async `AsyncDevice` API (native under `smol`/`tokio`, and
WebUSB on `wasm32`) — following the four-file pattern of `src/impls/hydrasdr/`
and `src/impls/hackrf/`, so FutureSDR's `Source`/`Sink` and
`AsyncSource`/`AsyncSink` both work with `bladerf1 = ["seify/bladerf1"]`.

## 1. What libbladerf-rs 0.5 gives us

* One API: every I/O method returns `impl MaybeFuture<Output = Result<T>>`.
  Sync adapter → `.wait()`, async adapter → `.await`
  (`use std::future::IntoFuture` not even needed; `.await` works directly).
* No runtime feature to forward. seify's `smol`/`tokio` features stay as they
  are; libbladerf-rs's async path works under either, and on wasm.
* Streams own their USB endpoint and buffer pool. `read`/`get_buffer`/
  `submit`/`recycle` never touch the device. Only `build`, `start`, `stop`,
  `close` need `&mut RfLinkSession` (i.e. the device). Awaited
  `read`/`get_buffer`/`wait_completion` ignore their `timeout` argument,
  consume at most one USB completion per await and are cancel-safe — exactly
  the contract seify's `with_timeout` relies on for hackrf/hydrasdr.
* `BladeRf1::from_first()/from_serial()` work on wasm (they list devices the
  page has been granted). `from_bus_addr` is native-only. `from_device`
  accepts an externally opened `nusb::Device`. `BladeRf1::close()` is the
  non-blocking shutdown; `Drop` does blocking I/O on native only.
* Pre-existing behaviour kept: no `Drop` on streams; a dropped stream leaves
  the module enabled until something closes it.

## 2. Layout

```
src/impls/bladerf1/
├── mod.rs           feature/cfg wiring + re-exports (mirrors hydrasdr/mod.rs)
├── common.rs        everything runtime-independent, shared by both halves
├── sync.rs          BladeRf, RxStreamer, TxStreamer  (cfg(not(wasm32)))
└── asynchronous.rs  AsyncBladeRf, AsyncBladeRfRxStreamer, AsyncBladeRfTxStreamer
                     (cfg(any(wasm32, smol, tokio)))
```

`src/impls/bladerf1.rs` is deleted (moved into `sync.rs`).

### 2.1 `common.rs`

Moved verbatim from today's `bladerf1.rs`:

* `BUFFER_SIZE`, `BUFFER_COUNT`, `INV_2048`, `INV_128`, `ch()`,
  `bladerf_err()`, all `convert_*` functions and `sign_extend_12`,
  `From<BladeRfRange(Item)> for Range(Item)`.

New:

* `DeviceSelector { First, Serial(String), BusAddr(String, u8), Fd(i32) }`
  and `device_selector(&Args)`: `fd` (linux) → `bus_id`+`address` (native)
  → `serial` → `First`. Both halves use it.
* `probe_descriptor(&nusb::DeviceInfo) -> Option<Args>`: native emits
  `driver=bladerf, serial=…, bus_id=…, address=…`; wasm emits
  `driver=bladerf, serial=…` (WebUSB has no bus topology). Serial comes from
  `DeviceInfo::serial_number()`.
* `RxConverter { format, pending: Option<(Buffer, usize)> }` with
  `fn drain_pending(&mut self, out) -> usize` and
  `fn consume(&mut self, buf: Buffer, out, recycle: impl FnMut(Buffer)) -> Result<usize>`
  so the "convert + carry-over" logic exists once and both `read`
  implementations are three lines.
* Constants for the WebUSB filter: `USB_VID = 0x2CF0`, `USB_PID = 0x5246`
  (re-exported from libbladerf-rs).

### 2.2 `sync.rs` (`cfg(not(target_arch = "wasm32"))`)

Today's `BladeRf`/`RxStreamer`/`TxStreamer` with:

* `.wait()` on every libbladerf-rs I/O call (`use libbladerf_rs::MaybeFuture`).
* `open()` routed through `device_selector`; `probe()` through
  `probe_descriptor`.
* `read()`/`write()` using the shared `RxConverter`/`convert_complex32_to_bytes`.
* `Drop for RxStreamer/TxStreamer`: best-effort `stop(&mut rf).wait()` if
  the stream was started (mirrors hydrasdr sync; today nothing is done and
  the RX module stays enabled after a drop).

Public API unchanged: `BladeRf::{probe, open, enable_expansion_board,
calibrate_dc}`, `RxStreamer`, `TxStreamer`.

### 2.3 `asynchronous.rs` (`cfg(any(wasm32, smol, tokio))`)

hydrasdr's shape, extended with TX:

```rust
#[derive(Clone)]
pub struct AsyncBladeRf {
    device_slot: Shared<AsyncSlot<Box<BladeRf1>>>,
    abandoned_rx: Shared<AsyncSlot<RxStream>>,
    abandoned_tx: Shared<AsyncSlot<TxStream>>,
    serial: String,
}

pub struct AsyncBladeRfRxStreamer {
    device_slot: Shared<AsyncSlot<Box<BladeRf1>>>,
    abandoned: Shared<AsyncSlot<RxStream>>,
    stream: Option<RxStream>,
    converter: RxConverter,
    active: bool,
}
// AsyncBladeRfTxStreamer: same fields minus converter, plus `format`.
```

* `AsyncSlot`/`AsyncSlotLease` copied from hydrasdr (Mutex on native,
  RefCell on wasm). Three copies now exist; factoring them into
  `async_compat` is a separate follow-up touching hackrf/hydrasdr.
* `lease_device()`: take the device out of the slot (`Error::Busy` if taken),
  then `cleanup_abandoned()`: for each parked stream, open an
  `rf_link_session().await` on the leased device and `close(&mut rf).await`
  it. This releases the streaming endpoint so a new streamer can be built.
* Every control op: `let mut dev = self.lease_device().await?;
  let mut rf = dev.value_mut().rf_link_session().await.map_err(bladerf_err)?;
  rf.xxx(..).await.map_err(bladerf_err)`. Bodies are the sync ones with
  `.await`.
* `async_rx_streamer` / `async_tx_streamer`: lease device (which also cleans
  up an abandoned stream of that direction), `RxStream::builder(&mut rf)
  ....build().await`, return the streamer holding clones of the slots.
* `activate_at(Some(_))` / `deactivate_at(Some(_))` →
  `Error::unsupported(Capability::TimedActivation/TimedDeactivation)` (the
  sync half sleeps for `time_ns`, which is not meaningful; async follows
  hydrasdr/hackrf). `activate_at(None)`: lease device → `stream.start(&mut rf).await`;
  idempotent when already active. `deactivate_at(None)`: lease →
  `stream.stop(&mut rf).await`.
* `read`: no device lease. Drain `converter.pending` first; then
  `with_timeout(stream.read(None), timeout_from_micros(timeout_us))`;
  `TimedOut → Ok(written)` (possibly 0). Convert into the caller's slice via
  `RxConverter::consume`, recycling or parking the DMA buffer as today.
* `write`: `with_timeout(stream.get_buffer(None), ..)`; `TimedOut →
  Err(Error::Timeout)`; convert, `submit`. `write_all`: loop `write`.
  `at_ns`/`end_burst` ignored as in the sync half.
* `Drop for Async*Streamer`: if `stream.is_some()`, park it in the abandoned
  slot (always, started or not — `close()` is harmless on a never-started
  stream and deconfigures the format GPIO bits). The next `lease_device()`
  closes it.
* `AsyncTypedDeviceBackend`: `driver()`, `webusb_filters()` →
  `[(0x2CF0, 0x5246)]` (+ serial filter when `serial=` given),
  `async_probe`, `async_open`.
* `impl_dyn_async_device_backend!(AsyncBladeRf => [rx, tx, antenna, agc,
  gain, frequency, sample_rate, bandwidth])`.
* `id()` returns the serial cached at open (no USB round trip);
  `info()` reads the FX3 firmware version through a lease.

### 2.4 Wiring

* `Cargo.toml`: move `libbladerf-rs` out of the `cfg(not(wasm32))` table
  into `[dependencies]` (it compiles for wasm now). Keep the local `path`
  during development; switch to `version = "0.5"` for release. No feature
  forwarding needed.
* `src/impls/mod.rs`: `pub mod bladerf1` under `feature = "bladerf1"`;
  re-export `BladeRf` under `not(wasm32)` and `AsyncBladeRf` under
  `any(wasm32, smol, tokio)` (same shape as hackrf/hydrasdr).
* `src/registry.rs`: unchanged.
* `src/async_registry.rs`: register `AsyncBladeRf` under
  `all(feature = "bladerf1", any(wasm32, smol, tokio))`; add
  `Driver::BladeRf` to the `unavailable_driver()` match; extend the
  ordering test and add `async_registry_reports_disabled_bladerf_without_runtime_feature`.
* `src/lib.rs` docs: mention bladeRF in the async/WebUSB paragraphs if the
  hackrf/hydrasdr ones are listed.
* `examples/webusb/Cargo.toml`: add `bladerf1 = ["seify/bladerf1"]` to the
  default features so the browser demo can open a bladeRF.
* `.github/workflows/ci.yml`: a `bladerf1` matrix job (`bladerf1`,
  `bladerf1,smol`, `bladerf1,tokio`) with clippy + test, and add `bladerf1`
  to the WebUSB clippy features. Note seify CI uses nightly clippy; lints
  only apply to seify's own code.
* `README.md`: list bladeRF 1 among the drivers with async/WebUSB support.

### 2.5 Tests

* `tests/bladerf1_sync_hardware.rs` (`#[ignore]`, `bladerf1 + not(wasm32)`):
  probe → open → rx channel controls → RX streamer read → TX streamer write →
  drop-without-deactivate recovery.
* `tests/bladerf1_async_hardware.rs` (`#[ignore]`, `bladerf1 +
  any(smol, tokio) + not(wasm32)`): the hydrasdr lifecycle test adapted:
  probe/open via `AsyncRegistry`, controls while a stream is active,
  zero-timeout read is cancel-safe, deactivate/reactivate keeps the queue,
  dropping an active streamer is recovered by the next `rx.streamer()`,
  plus a TX `write_all` round trip.
* Unit tests in `common.rs` for the converters (already-known vectors:
  full-scale Sc16Q11 ↔ Complex32, Sc8Q7, packed) and for
  `device_selector`/`probe_descriptor` parsing.
* Local run: `cargo test --no-default-features --features bladerf1,tokio --test bladerf1_async_hardware -- --ignored`
  and the same with `smol`, plus the sync test. Then
  `cargo clippy --target wasm32-unknown-unknown --lib --no-default-features --features=hydrasdr,bladerf1 -- -D warnings`.

## 3. Verification order

1. `cargo check --no-default-features --features bladerf1` (sync half compiles
   against libbladerf-rs 0.5).
2. `cargo test --no-default-features --features bladerf1` (unit tests, no
   hardware) → `--features bladerf1,tokio` → `--features bladerf1,smol`.
3. Hardware: sync test, then async test under tokio and smol.
4. wasm: clippy for the lib and for `examples/webusb` with `bladerf1`.
5. FutureSDR smoke: `cargo build --features bladerf1` in `../FutureSDR`
   (uses crates.io seify 0.23 today; a path override is needed to test the
   new backend — out of scope unless requested).
6. Browser validation of stream teardown on WebUSB (libbladerf-rs plan §2.6):
   build `examples/webusb` with trunk, open a bladeRF, activate/deactivate
   twice, drop an active streamer and reopen. This is the one behaviour that
   cannot be verified natively.

## 3b. Implementation notes

* Verified on hardware: `tests/bladerf1_sync_hardware.rs`, and
  `tests/bladerf1_async_hardware.rs` under both `tokio` and `smol`.
* Two latent bugs in the old sync backend fixed on the way: `TxStreamer::write`
  indexed a zero-length DMA buffer (`get_buffer` returns a cleared buffer);
  and a dropped streamer never disabled the RF module (now best-effort
  `close()` in `Drop`).
* TX never transmits bytes that did not come from caller samples: the
  converter's sample count is checked before `submit`, and libbladerf-rs
  `TxStream::submit` now rejects `len != buf.len()`.
* `tokio` is a dev-dependency (`rt`, `time`) so the async hardware test can
  build a runtime; bladeRF does not pull `nusb/tokio` the way hydrasdr does.
* `Cargo.toml` still points `libbladerf-rs` at `../libbladerf-rs` (TODO
  comment); switch to `version = "0.5"` once published.

## 4. Follow-ups (not in this change)

* Factor `AsyncSlot`/`AsyncSlotLease` out of the three backends into
  `async_compat`.
* Sync `activate_at(Some(t))` sleeping for `t` nanoseconds is legacy
  behaviour; consider `Capability::TimedActivation` unsupported there too.
* Expose `enable_expansion_board`/`calibrate_dc` on `AsyncBladeRf` once the
  typed async device API grows a way to reach backend-specific methods.
