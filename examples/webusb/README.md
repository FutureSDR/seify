# Seify WebUSB example

This is a framework-free Trunk page for exercising Seify's asynchronous WebUSB
API. It opens the first available device from a button click, discovers the
receiver controls exposed by that device, captures a finite block of IQ
samples, and plots their magnitude with the browser's 2D canvas API.

Run it from this directory:

```bash
trunk serve --open
```

`Trunk.toml` selects Cargo's release profile by default.

WebUSB requires a secure context; `localhost` served by Trunk qualifies. Use a
browser with WebUSB support. The first **Open SDR** click requests device
permission.

The application code contains no driver-specific selection or controls. Its
manifest enables the HackRF, HydraSDR, and RTL-SDR WebUSB backends; any can be
selected without changing the page logic.
