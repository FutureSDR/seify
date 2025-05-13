#![allow(clippy::empty_line_after_doc_comments)]

// A device that can have simultaneous tx and rx streams
// eg BladeRf
trait FullDuplexDevice {}

// A device that can have either a tx or rx streamer configured, but not at the same time
// eg HackRfOne
trait HalfDuplexDevice {}

// A transmit only device
// eg Osmo-FL2K (I don't think there are any serious devices)
trait SimplexDeviceTx {}

// A recieve only device
// eg RTL-SDR
trait SimplexDeviceRx {}

// A device that is half duplex, but can rapidly switch between TX and RX (do any exist?)
// Not sure how fast things like the HackRfOne and AirSpy can switch.
trait TDDDevice {}

// Any of the above devices will implement traits for any device.
// This just represents the trait that already exists in seify.
trait AnyDevice {}

/// Channels
////////////////////////////

// Independent channels?
// Locked together channels?

/// Streamers
////////////////////////////

// Streamers that have some fixed point datatype.
// will automatically implement the streamer trait.
trait FixedStreamerRx {}
trait FixedStreamerTx {}

/// Other Features
///////////////////////////////////

trait AgcDevice {}
