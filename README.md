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
cargo check --features hydrasdr,hackrfone
```

Available features:

| Feature | Driver argument | Notes |
| --- | --- | --- |
| `dummy` | `driver=dummy` | In-process test driver. |
| `soapy` | `driver=soapy` | SoapySDR backend. Enabled by default. Requires SoapySDR system libraries. |
| `aaronia_http` | `driver=aaronia_http` | Aaronia HTTP backend. |
| `bladerf1` | `driver=bladerf` | bladeRF 1 backend. |
| `hackrfone` | `driver=hackrfone` | HackRF One backend. |
| `hydrasdr` | `driver=hydrasdr` | HydraSDR backend. |
| `rtlsdr` | `driver=rtlsdr` | RTL-SDR backend. |

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
use seify::Device;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = Device::new()?;
    let rx0 = dev.rx(0)?;
    let mut samps = [Complex32::new(0.0, 0.0); 1024];
    let mut rx = rx0.streamer()?;
    rx.activate()?;
    let n = rx.read(&mut [&mut samps], 200000)?;
    println!("read {n} samples");

    Ok(())
}
```
