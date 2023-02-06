# Seify! A Rusty SDR Hardware Abstraction Library

## Goals

A clear path towards a great Rust SDR driver ecosystem.

- Seify has an implementation for Soapy and, therefore, supports basically all available SDR frontends.
- Seify supports both typed and generic devices with dynamic dispatch. There is no or minimal overhead for the typed version, i.e., there should be no reason not to use Seify.
- Once more native Rust drivers become available, they can be added to Seify and gradually move from Soapy to pure-Rust drivers.
- A clear path towards a proper async and WASM WebUSB.
- Zero-installation: Rust drivers need no libraries from the base system. Either they are network/http-based or they use `rusb`, which vendors `libusb`.
- Proper driver integration for Rust drivers (e.g., no threads in the core library).
- Rust drivers are added with crate features per binary and do not rely on system-wide libraries.  
- Provide a framework for Rust SDR drivers, to avoid diverging concepts of driver implementations in the ecosystem.

## Hardware Drivers

To add a new SDR driver, add a struct, implementing the `DeviceTrait` in the `src/impls` folder and add feature-gated logic for the driver to the probing/enumeration logic in `src/device.rs`.

At the moment, Seify is designed to commit the driver implementations upstream, i.e., there is no plugin system.
This will probably be added but is no priority at the moment.
While this concentrates maintenance efforts on Seify, it simplifies things for the user, who just add Seify to the project and enables feature flags for their SDR.

## Example

```rust
use num_complex::Complex32;
use seify::Device;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev = Device::new()?;
    let mut samps = [Complex32::new(0.0, 0.0); 1024];
    let mut rx = dev.rx_streamer(&[0])?;
    rx.activate()?;
    let n = rx.read(&mut [&mut samps], 200000)?;
    println!("read {n} samples");

    Ok(())
}
```
