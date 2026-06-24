# Core API Design Recommendations

This document collects large-scale structural recommendations for Seify's core abstractions. The focus is on getting the important SDR concepts right rather than making incremental cleanup changes.

## 1. Split the old monolithic device trait into capability traits

The core device API used to be a large mandatory interface covering metadata, antennas, AGC, gain, frequency, sample rate, bandwidth, DC offset, RX streaming, and TX streaming.

That makes every backend implement the full surface area, even when many methods simply return `Error::NotSupported`.

A better structure would be capability-oriented:

```rust
trait DeviceInfo { ... }
trait ChannelInfo { ... }

trait RxDevice { type RxStreamer; fn rx_streamer(...); }
trait TxDevice { type TxStreamer; fn tx_streamer(...); }

trait AntennaControl { ... }
trait GainControl { ... }
trait AgcControl { ... }
trait FrequencyControl { ... }
trait SampleRateControl { ... }
trait BandwidthControl { ... }
trait DcOffsetControl { ... }
```

The high-level `Device` facade can still expose ergonomic methods, but the core model should represent what the hardware actually supports.

Benefits:

- Less fake uniformity.
- Fewer dummy methods.
- Better compile-time expression of RX-only/TX-only devices.
- Easier addition of new capabilities without breaking every backend.
- Clearer distinction between unsupported, absent, and unavailable functionality.

## 2. Introduce explicit channel types

Most methods currently take:

```rust
(direction: Direction, channel: usize)
```

This is simple but not semantically rich. Prefer first-class channel identifiers:

```rust
struct Channel {
    direction: Direction,
    index: usize,
}
```

or separate types:

```rust
struct RxChannel(usize);
struct TxChannel(usize);
```

Then APIs can move from:

```rust
device.set_frequency(Direction::Rx, 0, 100e6)?;
device.set_gain(Direction::Tx, 0, 10.0)?;
```

towards:

```rust
device.rx(0)?.set_frequency(Hz(100e6))?;
device.tx(0)?.set_gain(Db(10.0))?;
```

This reduces repeated arguments and makes invalid combinations harder to express.

## 3. Model the device/channel/component hierarchy

The current API still flattens many controls onto device-level methods. SDR hardware is more naturally hierarchical:

```text
Device
  ├── RX channel 0
  │     ├── antenna
  │     ├── gain stages
  │     ├── frequency components
  │     ├── sample rate
  │     └── bandwidth
  ├── RX channel 1
  └── TX channel 0
```

A more expressive API could look like:

```rust
let rx0 = device.rx(0)?;
rx0.set_frequency(Hz(100e6))?;
rx0.set_sample_rate(SamplesPerSecond(2e6))?;
rx0.gain("LNA")?.set(Db(20.0))?;
```

This makes capabilities local to the thing they configure instead of passing `direction` and `channel` everywhere.

## 4. Separate driver backend, discovery, and opened device

An opened device is represented by `Device<T>` or `Device<DynDevice>`, but probing/opening logic is currently tied into `Device<DynDevice>::from_args()` and `enumerate_with_args()` using feature-gated blocks.

Introduce a driver registry abstraction:

```rust
trait DriverBackend {
    fn driver(&self) -> Driver;
    fn probe(&self, args: &Args) -> Result<Vec<DeviceDescriptor>>;
    fn open(&self, descriptor: &DeviceDescriptor) -> Result<AnyDevice>;
}
```

This separates:

- discovering hardware,
- opening hardware,
- controlling an opened device.

A user-facing flow could be:

```rust
let registry = Registry::default();
let devices = registry.probe(args)?;
let dev = registry.open(&devices[0])?;
```

`Device::new()` and `Device::from_args()` can remain convenience APIs on top.

## 5. Replace stringly typed `Args` in core operations with typed config structs

`Args` is useful for discovery strings and vendor-specific escape hatches, but core operations should not primarily depend on string keys.

Current examples:

```rust
rx_streamer(&self, channels: &[usize], args: Args)
set_frequency(..., args: Args)
```

Prefer typed request/config structs:

```rust
struct StreamConfig {
    format: SampleFormat,
    buffer_size: Option<usize>,
    buffer_count: Option<usize>,
    vendor: Args,
}

struct TuneRequest {
    frequency: Hz,
    rf_offset: Option<Hz>,
    components: Vec<ComponentTune>,
    vendor: Args,
}
```

`Args` remains available as `vendor`, `extra`, or `driver_args`, but common behavior becomes documented and type-checked.

## 6. Use units as types

The core API uses raw `f64`/`i64` for Hz, dB, bandwidth, sample rate, and timeouts. Consider lightweight newtypes:

```rust
struct Hz(f64);
struct Db(f64);
struct SamplesPerSecond(f64);
struct Nanoseconds(i64);
struct Microseconds(i64);
```

This makes signatures self-documenting:

```rust
fn set_frequency(&self, freq: Hz) -> Result<()>;
fn set_gain(&self, gain: Db) -> Result<()>;
fn read(&mut self, buffers: ..., timeout: Timeout) -> Result<ReadResult>;
```

It also prevents accidental unit mixups.

## 7. Redesign typed vs erased device abstraction

The current design uses:

```rust
Device<T>
type DynDevice = Arc<dyn DynDeviceBackend>;
DeviceWrapper<D>
```

This works but creates forwarding boilerplate and awkward downcasting.

Prefer separate public concepts:

```rust
pub struct Device<D> {
    inner: D,
}

pub struct AnyDevice {
    inner: Arc<dyn ErasedDevice>,
}
```

With explicit conversion:

```rust
impl<D: DeviceBackend> Device<D> {
    pub fn erase(self) -> AnyDevice;
}

impl AnyDevice {
    pub fn downcast_ref<D: DeviceBackend>(&self) -> Option<&D>;
}
```

This makes the model clearer:

- `Device<Dummy>` is typed.
- `AnyDevice` is type-erased.

Instead of making `Device<DynDevice>` a special case.

## 8. Remove the blanket `Clone` requirement from devices

Currently:

```rust
pub struct Device<T> {
    dev: T,
}
```

Requiring every device implementation to be `Clone` is a strong constraint. Many hardware handles are not naturally cloneable. If clones are cheap shared references, that should be a driver implementation detail.

Prefer:

```rust
pub struct Device<T> {
    dev: T,
}
```

If shared ownership is needed, use `Arc<T>` explicitly or make streamers hold an internal `Arc<Inner>`.

## 9. Reconsider `&self` mutation semantics

Most configuration methods currently take `&self`:

```rust
fn set_gain(&self, ...)
fn set_frequency(&self, ...)
fn set_sample_rate(&self, ...)
```

This implies every driver uses interior mutability.

Two possible directions:

### Option A: use `&mut self` for configuration

This is more Rust-like and statically prevents concurrent conflicting configuration changes.

```rust
fn set_frequency(&mut self, ...)
```

### Option B: keep `&self`, but make synchronization explicit

If device handles are intentionally internally synchronized, the trait bounds and documentation should say so. In that case, the capability and dynamic backend traits probably need clear `Send`/`Sync` semantics.

Right now the runtime-dispatched backend requires `Send + Sync`. That should be documented clearly if configuration continues to mutate through `&self`.

## 10. Give streamers richer metadata and results

`RxStreamer` and `TxStreamer` are currently minimal: MTU, activation, deactivation, read/write.

SDR streaming often needs more information:

- timestamp support,
- hardware time,
- overrun/underrun indicators,
- late packet flags,
- burst markers,
- discontinuity flags,
- active state,
- channel list,
- sample format.

Instead of:

```rust
read(...) -> Result<usize>
write(...) -> Result<usize>
```

consider:

```rust
struct ReadResult {
    samples: usize,
    timestamp: Option<DeviceTime>,
    flags: StreamFlags,
}

struct WriteResult {
    samples: usize,
    flags: StreamFlags,
}
```

This makes timing and stream errors first-class concepts.

## 11. Make sample format generic or explicit

The current streamer API hardcodes:

```rust
Complex32
```

That is ergonomic, but many devices natively use `i8`, `i16`, packed formats, or backend-specific formats. For performance-sensitive applications, forced conversion to `Complex32` may be undesirable.

Possible directions:

```rust
trait RxStreamer<S: Sample> {
    fn read(&mut self, buffers: &mut [&mut [S]], timeout: Timeout) -> Result<ReadResult>;
}
```

or use negotiated explicit formats:

```rust
enum SampleFormat {
    Complex32,
    Sc16,
    Sc8,
}
```

Conversion adapters can provide ergonomic `Complex32` streams on top of native streams.

## 12. Add structured capability descriptions

Currently users query capabilities through many individual methods:

```rust
gain_elements()
gain_range()
frequency_components()
frequency_range()
get_sample_rate_range()
get_bandwidth_range()
supports_agc()
has_dc_offset_mode()
```

Consider a coherent capability snapshot:

```rust
struct DeviceCapabilities {
    rx_channels: Vec<ChannelCapabilities>,
    tx_channels: Vec<ChannelCapabilities>,
}

struct ChannelCapabilities {
    antennas: Vec<Antenna>,
    gain: Option<GainCapabilities>,
    frequency: FrequencyCapabilities,
    sample_rates: Range<SamplesPerSecond>,
    bandwidths: Option<Range<Hz>>,
    dc_offset: DcOffsetCapabilities,
}
```

The individual methods can remain convenience wrappers, but applications benefit from a single description of the device shape.

## 13. Improve error semantics

The current error model has broad variants such as:

```rust
NotFound
NotSupported
ValueError
DeviceError
Misc(String)
```

Core errors should distinguish important SDR cases:

```rust
enum Error {
    Unsupported(Capability),
    InvalidChannel(Channel),
    InvalidArgument { name: String, reason: String },
    OutOfRange { value: Quantity, range: Range<Quantity> },
    DeviceDisconnected,
    Timeout,
    Overrun,
    Underrun,
    Driver(DriverError),
}
```

Applications need to distinguish unsupported bandwidth control from invalid channel index, timeout, device disconnection, out-of-range values, and backend-specific failures.

## 14. Make `Range` typed

Current ranges are untyped `f64` collections:

```rust
struct Range {
    items: Vec<RangeItem>,
}
```

Prefer:

```rust
struct Range<T> {
    items: Vec<RangeItem<T>>,
}
```

Then APIs can return:

```rust
Range<Hz>
Range<Db>
Range<SamplesPerSecond>
```

This pairs naturally with unit newtypes and makes capabilities more precise.

## 15. Split the core modules by concept

`device.rs` currently contains the central trait, typed wrapper, erased wrapper, forwarding implementations, downcasting, and public facade methods.

A clearer structure would be:

```text
src/core/
  device.rs          // device traits and opened-device concepts
  channel.rs         // channel abstractions
  capabilities.rs    // capability descriptions
  stream.rs          // streamer traits and stream results
  registry.rs        // probing/opening/driver registry
  erased.rs          // AnyDevice and erased adapters
  units.rs           // Hz, Db, time, sample-rate types
```

The public facade can stay ergonomic, but the internal concepts should be smaller and easier to reason about.

## Overall direction

The current API is shaped like a C-style universal device vtable: one large trait with many methods parameterized by direction and channel.

A stronger Rust design would move toward a hierarchical, capability-based model:

```text
Registry → DeviceDescriptor → Device → RxChannel/TxChannel → Controls + Streamers
```

with:

- typed channel handles,
- typed units,
- typed config structs,
- explicit capabilities,
- separate typed and erased device types,
- less reliance on `Args`,
- richer stream metadata,
- and fewer mandatory trait methods.
