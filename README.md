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
| `rtlsdr` | `driver=rtlsdr` | RTL-SDR backend using `rtlsdr-nusb`; native sync/async and WebUSB support. |
| `smol` / `tokio` | n/a | Pick one for async `nusb` runtime integration. |

For native async use with `nusb`-based drivers, enable exactly one of `smol` or
`tokio`. For example, native HackRF async support is enabled with `hackrf,smol`
or `hackrf,tokio`, and bladeRF 1 with `bladerf1,smol` or `bladerf1,tokio`.
The bladeRF 1 backend needs one of the two even for synchronous use, because
libbladerf-rs resolves nusb's blocking USB operations through the selected
runtime. WebAssembly uses WebUSB and needs only the corresponding driver
feature.

## WebUSB

HackRF, HydraSDR, bladeRF 1, and RTL-SDR are available on `wasm32-unknown-unknown`. Only
`AsyncHackRf`, `AsyncHydraSdr`, `AsyncBladeRf`, `AsyncRtlSdr`, `AsyncRegistry`, and the async
device/streamer APIs are connected to those drivers on wasm; their synchronous
backends remain native-only.

Build it with:

```bash
cargo check --target wasm32-unknown-unknown --no-default-features --features hackrf,hydrasdr,bladerf1,rtlsdr
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
