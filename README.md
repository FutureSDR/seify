# Seify! A Rusty SDR Hardware Abstraction Library

## Goal

A clear path towards a great Rust SDR driver ecosystem.

- Seify has an implementation for Soapy and, therefore, supports basically all available SDR frontends.
- Seify supports both typed and generic devices with dynamic dispatch. Using the typed interface
- Once more native Rust drivers become available, they can be added to Seify and gradually move from Soapy to Rust.
- A clear path towards a proper async and WASM WebUSB.

## Hardware Drivers

To add new SDRs driver, just add a new struct, implementation the `DeviceTrait` in the `src/impls` folder and add feature-gated logic for the driver to the probing/enumeration logic in `src/device.rs`.


At the moment, Seify is designed to commit the driver implementations upstream, i.e., there is no plugin system.
This could be easily implemented but is no priority at the moment.
Also, while this concentrates maintainance efforts on Seify, it has adavantages:
- the user just add Seify to the project and enables feature flags as needed

## Conventions

- name bidirectional antenna port "TRX"
