# HydraSDR RFOne implementation map

This is the compact integration map for adding an rx-only Seify driver backed by
`hydrasdr-rs`'s current direct synchronous API.  It intentionally avoids the
future ergonomic API layer mentioned in `hydrasdr-rs`; use only the sync methods
on `hydrasdr_rs::device::HydraSdr` and `hydrasdr_rs::discovery`.

## Sources inspected

Seify:

- `src/device.rs`: capability traits, `Device<T>`, `DynDevice`, feature-gated
  driver probing/opening.
- `src/streamer.rs`: `RxStreamer`/`TxStreamer` pull-stream contracts.
- `src/lib.rs`: `Error`, `Driver`, `Direction`, `enumerate_with_args`.
- `src/impls/rtlsdr.rs`: rx-only native-driver template.
- `src/args.rs` and `src/range.rs`: string args and range representation.

HydraSDR Rust driver:

- `../hydrasdr-rs/src/lib.rs`: public exports (`HydraSdr`, `Error`, `Result`,
  `StatusCode`).
- `../hydrasdr-rs/src/discovery.rs`: sync device listing and serial parsing.
- `../hydrasdr-rs/src/device.rs`: sync open/config/start/stop API.
- `../hydrasdr-rs/src/streaming.rs`: callback `Transfer` shape and direct
  streaming state.
- `../hydrasdr-rs/src/types.rs`, `commands.rs`, `rfone.rs`: sample types,
  gain selectors, RF ports, and RFOne limits.
- `../hydrasdr-rs/examples/direct_sync.rs`: intended sync API usage pattern.

## Cargo and feature shape

Add a Seify feature named `hydrasdr`:

- `hydrasdr = ["dep:hydrasdr-rs"]`
- target dependency: `hydrasdr-rs = { path = "../hydrasdr-rs", optional = true }`
- implementation module: `src/impls/hydrasdr.rs`
- `impls/mod.rs`: gate and re-export `HydraSdr` under the `hydrasdr` feature.
- `lib.rs`: add `Error::HydraSdr(#[from] hydrasdr_rs::Error)` under the same
  target/feature gate, add `Driver::HydraSdr`, and parse aliases `hydrasdr`,
  `hydrasdr-rs`, `hydra`, and `rfone`.
- `enumerate_with_args` and `Device<DynDevice>::from_args` get the same
  feature-gated blocks as `RtlSdr`/`HackRfOne`.

## Device identity and args

Supported args for the first pass:

- `driver=hydrasdr` selects this driver.
- `serial=<u64>` opens a specific RFOne with `hydrasdr_rs::device::HydraSdr::open_sn`.
- no `serial` opens the first visible RFOne with `HydraSdr::open`.

Do not support `index` in the first pass. `hydrasdr-rs` exposes sync `open()` and
`open_sn(u64)`, but no sync open-by-index surface. `probe()` can still enumerate
all visible devices with `hydrasdr_rs::discovery::list_devices()` and return args
like:

```text
driver=hydrasdr, serial=<decimal u64>, vid=<hex>, pid=<hex>, product=<string>
```

Only `driver` and `serial` should be required to re-open a device. The USB serial
string is parsed by `hydrasdr-rs` as `Option<u64>`; keep Seify's `serial` value in
that parsed decimal form unless a later task adds custom hex parsing.

`id()` should prefer the decimal serial when known. If the device was opened
without a serial and the USB descriptor did not provide one, query
`board_partid_serialno_read()` and format the four firmware serial words as a
stable hex string. `info()` should include `driver=hydrasdr` and whichever serial
form `id()` uses.

## Rx-only Seify behavior

Mirror the rx-only shape of `src/impls/rtlsdr.rs`:

- `num_channels(Rx) -> Ok(1)`, `num_channels(Tx) -> Ok(0)`.
- channel `0` is the only valid Rx channel; other Rx channels are
  `Error::ValueError`.
- all Tx methods/queries are `Error::NotSupported`.
- `full_duplex(...) -> Ok(false)`.
- `tx_streamer(...) -> Err(Error::NotSupported)` and the associated Tx streamer
  can be an unreachable dummy, as in the RTL-SDR implementation.

## RF input / antenna mapping

Expose HydraSDR RFOne's selectable RF ports as Seify antennas:

- `ANT` -> `hydrasdr_rs::commands::RfPort::Rx0`
- `CABLE1` -> `RfPort::Rx1`
- `CABLE2` -> `RfPort::Rx2`

`hydrasdr-rs` currently provides `set_rf_port` but no getter, so the Seify driver
should cache the selected antenna in its `Inner` state. Default the cache to
`ANT`/`Rx0`; `set_antenna(Rx, 0, name)` updates hardware and cache. Unknown names
are `ValueError`.

## Frequency, sample rate, bandwidth, and gain

Frequency:

- `frequency_range(Rx, 0)` and `component_frequency_range(..., "TUNER")` use the
  RFOne constants exposed in `rfone.rs`: 24 MHz to 1.8 GHz.
- `set_frequency`/`set_component_frequency` call `HydraSdr::set_freq(frequency as u64)`.
- `hydrasdr-rs` has no frequency getter, so cache the last set frequency in
  `Inner`. Initialize it to `None`; `frequency(...)` should return cached value
  when set, otherwise `Error::NotSupported` rather than inventing a default.

Sample rate:

- On open, call `get_samplerates()` if possible and cache the returned discrete
  list.
- `get_sample_rate_range(Rx, 0)` returns `RangeItem::Value(rate as f64)` for each
  cached rate. If the device returns no list, fall back to an interval starting at
  10 kS/s, matching `hydrasdr-rs`'s sync `set_samplerate` lower bound.
- `set_sample_rate` calls `HydraSdr::set_samplerate(rate as u32)` and caches the
  selected rate. `sample_rate(...)` returns the cache if set; otherwise, return
  the first cached rate if available, else `Error::NotSupported`.

Bandwidth:

- `hydrasdr-rs` has `get_bandwidths()` and `set_bandwidth(u32)`. Implement Seify
  bandwidth methods using cached discrete `RangeItem::Value` entries if the list
  is available.
- If no bandwidth list is available, `get_bandwidth_range` can return
  `Error::NotSupported`; do not guess analog filter ranges.

Gain:

- Use Seify gain elements `LNA`, `MIXER`, `VGA`, `LINEARITY`, and `SENSITIVITY`,
  mapped to `GainType::{Lna,Mixer,Vga,Linearity,Sensitivity}`.
- `gain_element_range` should come from `HydraSdr::get_all_gains()`/`get_gain()`
  descriptors (`min_value`, `max_value`, `step_value`) and use a step range.
- `set_gain_element` calls `HydraSdr::set_gain(gain_type, gain.round() as u8)`
  after range validation, then updates the cache.
- Overall `set_gain` should use `LINEARITY` as the first-pass aggregate gain,
  because RFOne exposes it as a preset across LNA/mixer/VGA and it has a single
  bounded knob. Overall `gain`/`gain_range` delegate to `LINEARITY`.
- `supports_agc(Rx, 0) -> Ok(true)` because LNA and mixer AGC toggles exist.
  `enable_agc(true)` sets both `GainType::LnaAgc` and `GainType::MixerAgc` to
  `1`; `enable_agc(false)` sets both to `0`; cache the requested mode.

## Streamer and sample conversion

Seify's `RxStreamer::read` is pull based and returns `Complex32`; `hydrasdr-rs`'s
sync streaming API is callback based:

```rust
dev.start_rx(|transfer: &Transfer<'_>| { ...; nonzero_to_stop })
```

First-pass mapping:

- Store the direct `HydraSdr` handle behind `Arc<Mutex<_>>` because almost all
  sync configuration and streaming methods require `&mut self`.
- On `rx_streamer(&[0], args)`, set `SampleType::Float32Iq` and `set_packing(0)`.
  Packed mode is intentionally unsupported for Seify v1 because it complicates
  byte-to-`Complex32` conversion and is not needed for correctness.
- `activate_at(None)` marks the streamer active. Timed activation is unsupported:
  `activate_at(Some(_)) -> Error::NotSupported`.
- `read(...)` validates one output buffer, then calls `start_rx` and copies from
  callback `Transfer.samples` into the destination until the destination is full
  or the first transfer ends. Return non-zero from the callback to stop the direct
  streaming loop. Use raw byte length, not `Transfer.sample_count`, when decoding.
- Decode `SampleType::Float32Iq` as little-endian interleaved `f32` pairs:
  every 8 bytes become `Complex32::new(i, q)`. Ignore trailing incomplete chunks.
- Keep a small leftover `Vec<Complex32>` inside the streamer if a transfer yields
  more decoded samples than the caller's destination slice.
- `deactivate_at(None)` calls `HydraSdr::stop_rx()` if a stream is active enough
  to need shutdown, then marks inactive. Timed deactivation is unsupported.
- `timeout_us` cannot be faithfully applied with the current public sync API:
  `hydrasdr-rs` uses its internal direct streaming timeout. Document this in the
  implementation and treat negative/zero timeout like the rest of Seify's native
  drivers unless the sync API later exposes a configurable timeout.

`mtu()` should initially return the default decoded Float32 IQ capacity from
`hydrasdr-rs`'s default raw USB buffer size (`262_144 / 8 = 32_768` complex
samples). Keep this constant local to the Seify driver rather than depending on a
private `hydrasdr-rs` constant.

## Unsupported / no-hardware behavior

No hardware is attached on the coder host, so the first implementation should be
verified with no-hardware checks only:

- `cargo fmt --all -- --check`
- `cargo test --no-default-features --features dummy`
- `cargo check --no-default-features --features hydrasdr`

Hardware smoke instructions should stay gated/manual, for example:

```bash
cargo test --no-default-features --features hydrasdr -- --ignored
# or a later example binary that opens driver=hydrasdr,serial=<u64>
```

Do not block merely because real RX cannot be exercised here. Block only if the
sync API above is missing/mismatched, compile checks fail, or a hardware-specific
question is needed before changing public behavior.
