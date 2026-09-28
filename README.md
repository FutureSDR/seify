# Seify

Rust SDR hardware abstraction for applications that want one API over multiple
radio backends.

## What Seify Provides

A clear path towards a great Rust SDR driver ecosystem.

- One API for probing, opening, configuring, and streaming from SDR devices.
- Typed devices when an application wants a concrete backend.
- Type-erased devices when an application wants runtime driver selection.
- Capability-oriented channel APIs, so backends expose the controls they support.
- Feature-gated drivers, so each binary only includes the SDR backends it needs.
- SoapySDR support for broad hardware coverage and native Rust drivers where available.

The native Rust drivers are still experimental. For production use and the
widest set of stable hardware integrations, prefer the SoapySDR backend.

## Features

The default feature set is `soapy`.

Enable drivers explicitly in `Cargo.toml` or on the command line:

```bash
cargo check --no-default-features --features rtlsdr
cargo check --no-default-features --features hydrasdr,hackrf
```

Available features:

| Feature | Driver argument | Notes |
| --- | --- | --- |
| `dummy` | `driver=dummy` | Driver for unit tests. |
| `soapy` | `driver=soapy` | SoapySDR backend. Enabled by default. Requires SoapySDR system libraries. |
| `aaronia_http` | `driver=aaronia_http` | Aaronia HTTP backend. |
| `bladerf1` | `driver=bladerf` | Full-duplex bladeRF 1 RX/TX backend; requires `smol` or `tokio` on native targets; async WebUSB support on `wasm32-unknown-unknown`. |
| `hackrf` | `driver=hackrf` | Half-duplex HackRF RX/TX backend; async WebUSB support on `wasm32-unknown-unknown`. |
| `hydrasdr` | `driver=hydrasdr` | HydraSDR backend; async WebUSB support on `wasm32-unknown-unknown`. |
| `pluto` | `driver=pluto` | Native PlutoSDR IIO USB RX and configuration; sync, async, and WebUSB. TX is not implemented. |
| `rtlsdr` | `driver=rtlsdr` | RTL-SDR backend using `rtlsdr-nusb`; native sync/async and WebUSB support. |
| `uhd` | `driver=uhd` | USRP B2xx channel-zero RX using `uhd-rs`; native sync/async and WebUSB support. |
| `smol` / `tokio` | n/a | Pick one for async `nusb` runtime integration. |

For native async use with `nusb`-based drivers, enable exactly one of `smol` or
`tokio`. For example, native HackRF async support is enabled with `hackrf,smol`
or `hackrf,tokio`, and bladeRF 1 with `bladerf1,smol` or `bladerf1,tokio`.
The bladeRF 1 backend needs one of the two even for synchronous use, because
libbladerf-rs resolves nusb's blocking USB operations through the selected
runtime. WebAssembly uses WebUSB and needs only the corresponding driver
feature.

## PlutoSDR

The `pluto` backend uses the published Rust `plutosdr-rs` driver from crates.io.
No libiio, libusb, SoapySDR, or USB Ethernet transport is involved in this backend.

Enable `pluto` for `impls::Pluto`, `pluto,smol` or `pluto,tokio` for native
`impls::AsyncPluto`, or just `pluto` for WebUSB. Driver aliases are `pluto`,
`plutosdr`, and `adalm-pluto`. `serial` preserves its exact USB spelling and
leading zeros; `index` takes precedence when both are present.

```sh
cargo run --no-default-features --features pluto --example probe -- --args driver=pluto
# Explicit hardware test, including native async when smol is enabled:
cargo test --no-default-features --features pluto,smol --test pluto_hardware -- --ignored --nocapture
```

`Registry` and `AsyncRegistry` can probe and open the device. `info()` includes
USB identity, reported board/firmware, IIO context metadata, and an `iio_devices`
JSON array. Typed backends expose the complete cached XML model through
`context()`.

One RX channel provides frequency, sample rate, RF bandwidth, gain, AGC, and
antenna/port controls. All numeric ranges come from firmware. Setting gain
selects manual mode; enabling Seify AGC selects slow attack. The standalone Pluto driver also exposes fast-attack/hybrid modes. The Seify gain element
is `RX`, and the frequency component is `RF`. Port names are internal AD936x
inputs; Pluto has one physical RX connector. TX and timed activation are unsupported.

```rust,ignore
let device = seify::Device::<seify::impls::Pluto>::from_args("driver=pluto")?;
let channel = device.rx(0)?;
channel.frequency().set(2_450_000_000.0)?;
channel.sample_rate().set(2_500_000.0)?;
channel.bandwidth().set(2_000_000.0)?;
channel.gain().set(30.0)?;
let mut rx = device.rx_streamer(&[0])?;
rx.activate()?;
```

Run `cargo run --no-default-features --features pluto --example rx_generic -- --args driver=pluto` for a
complete capture example. Async channel creation and controls use `.await`.
`rx_streamer_with_args` accepts `buffer_samples` (default 65,536 complex frames).
The RX stream returns normalized `Complex32`, retains block tails across short
reads, and uses a separate USB pipe from controls. Negative read timeouts use the
driver's default three seconds; zero returns cached samples or Timeout. A failed
or cancelled exchange requires deactivation before reactivation. No FIR loading,
resampling, timestamps, or reliable sample-loss reporting is implemented.

Clones share one control session. Stream handles own their RX pipe and may
outlive device handles. Drop all streams before calling
`device.as_inner().shutdown()` (await it for `AsyncPluto`); otherwise it returns
Busy, including for stopped stream handles. Successful shutdown releases the
shared control session while keeping cached metadata readable. Stop RX before
retuning if samples from the previous configuration must be discarded.
Browser cleanup is asynchronous; explicit deactivation is preferred.

## WebUSB

HackRF, HydraSDR, bladeRF 1, PlutoSDR, RTL-SDR, and UHD are available on
`wasm32-unknown-unknown`. Only `AsyncHackRf`, `AsyncHydraSdr`, `AsyncBladeRf`,
`AsyncPluto`, `AsyncRtlSdr`, `AsyncUhd`, `AsyncRegistry`, and the async
device/streamer APIs are connected to those drivers on wasm; their synchronous
backends remain native-only.

Build it with:

```bash
cargo check --target wasm32-unknown-unknown --no-default-features --features hackrf,hydrasdr,bladerf1,pluto,rtlsdr,uhd
```

WebUSB's `web-sys` bindings require `--cfg=web_sys_unstable_apis`; this
repository supplies it for `wasm32-unknown-unknown` in `.cargo/config.toml`.
Applications consuming Seify as a dependency must add the same target setting
to their own Cargo configuration. A browser only probes devices already
authorized for the page. Call `AsyncRegistry::request_permission` from a browser
user gesture, then probe or open the authorized device in the window or a Web
Worker. Without a `driver` argument, the chooser includes devices supported by
all registered WebUSB backends. Opening itself never displays the chooser.

A framework-free, driver-agnostic browser example with discovered receiver
controls and a finite-capture magnitude plot lives in `examples/webusb`. Run it
with:

```bash
cd examples/webusb
trunk serve --open
```

The HackRF backend exposes RX channel 0, the single `ANT` port, 1 MHz–6 GHz
tuning, 2–20 Msample/s rates, and physical `AMP`, `LNA`, and `VGA` gain elements.
Its baseband-filter bandwidth follows the selected sample rate and is not a
separate Seify capability.

The RTL-SDR backend supports R820T-family and R828D tuners through `rtlsdr-nusb`.
Use `rtlsdr,smol` or `rtlsdr,tokio` for native async operation. It exposes RX
channel 0, the `RX` antenna, automatic/manual `TUNER` gain in dB (0–52.5), and
900,001–3,200,000 samples/s. Tuning spans 28.8 MHz–1.766 GHz; Blog V4 devices
also expose HF tuning. The driver reports quantized frequency/sample rates,
and reads return converted complex samples with partial-read and timeout support.
Standalone bandwidth control and other tuner families are not supported by this
driver. `index` takes precedence over `serial`; serials retain their USB spelling
and leading zeros.

The `uhd` backend uses [uhd-rs](https://github.com/bastibl/uhd-rs) without C++
UHD or system USB libraries. Enable `uhd` for synchronous `impls::Uhd`, or
`uhd,smol` / `uhd,tokio` for native `impls::AsyncUhd`. WebUSB needs only `uhd`.
Firmware and FPGA images are embedded by default. The optional dependency is
GPL-3.0-or-later; see its license for alternative licensing terms.

Radio support covers B200 (including revisions before 5), B210, B200mini,
and B205mini. It exposes RX channel 0, antenna `RX2`, 70 MHz–6 GHz tuning,
and manual `PGA` gain of 0–76 dB in 1 dB steps or AGC.
Sample rates up to 16 MS/s use a 16 MHz clock with supported integer DDC
decimation (31,250–16,000,000 samples/s). Requests above 16 MS/s, up to 20 MS/s,
select a 20 MHz clock and deliver 20 MS/s. Stop the stream before switching
clock modes. Getters return actual quantized rates and frequencies. Frequency arguments accept `lo_offset` in Hz.
TX, timed streaming, and independent bandwidth control are unavailable.
B210 channel 0 is the A-side receiver. Model-specific RF routing and gain/AGC
controls follow the board wiring; the second B210 channel is not yet exposed.
Opened-device metadata includes the motherboard `model` and `revision`, even
when a B210 USB descriptor identifies itself as B200.

The backend uses the published `uhd-rs` 0.1.2 release from crates.io.

Select with `driver=uhd,serial=...` (serials preserve leading zeros), or `index=N`
which takes precedence. A stream must be activated before reading. Reads may
be partial and return `Error::Timeout` when no samples arrive; a negative timeout
waits indefinitely. Only one stream can be claimed at a time. Typed streams expose
`close()` and typed backends expose `shutdown()` for explicit cleanup. In a
browser, restart a stopped stream to reuse it; after closing a stream that
submitted transfers, shut down and reopen the device before claiming another.
Firmware re-enumeration may require another user gesture for WebUSB permission.

```bash
cargo run --no-default-features --features uhd --example probe -- --args driver=uhd
cargo run --no-default-features --features uhd --example rx_generic -- --args driver=uhd
```

Use the generic API with an argument string to select a backend at runtime:

```bash
cargo run --no-default-features --features rtlsdr --example probe -- --args driver=rtlsdr
cargo run --no-default-features --features rtlsdr --example rx_generic -- --args driver=rtlsdr
```

Additional driver-specific arguments can be passed in the same string:

```bash
cargo run --no-default-features --features soapy --example probe -- --args driver=soapy,soapy_driver=rtlsdr
```

## Example

```rust
use num_complex::Complex32;
use seify::DynDevice;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = DynDevice::new()?;
    let rx0 = dev.rx(0)?;
    let mut samps = [Complex32::new(0.0, 0.0); 1024];
    let mut rx = rx0.streamer()?;
    rx.activate()?;
    let n = rx.read(&mut [&mut samps], 200000)?;
    println!("read {n} samples");

    Ok(())
}
```
