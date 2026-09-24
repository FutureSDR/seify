#![cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]

use num_complex::Complex32;
use seify::{Capability, Error, Registry, RxStreamer, TxStreamer};

const TIMEOUT_US: i64 = 2_000_000;

#[test]
#[ignore = "requires an attached bladeRF 1"]
fn sync_bladerf_lifecycle() -> Result<(), Error> {
    let registry = Registry::default();
    let descriptors = registry.probe("driver=bladerf")?;
    let descriptor = descriptors.first().ok_or(Error::DeviceNotFound)?;
    assert!(descriptor.args().get::<String>("serial").is_ok());
    let device = registry.open(descriptor)?;
    assert_eq!(device.id()?, descriptor.args().get::<String>("serial")?);

    let rx = device.rx(0)?;
    let tx = device.tx(0)?;
    rx.frequency().set(915e6)?;
    assert!((rx.frequency().value()? - 915e6).abs() < 1e3);
    rx.sample_rate().set(4e6)?;
    assert!((rx.sample_rate().value()? - 4e6).abs() < 1e3);
    rx.agc().disable()?;
    assert!(!rx.agc().enabled()?);
    rx.gain().set(30.0)?;
    assert!((rx.gain().value()?.unwrap() - 30.0).abs() <= 3.0);
    assert!(!rx.gain().elements()?.is_empty());
    let rx_bandwidth = rx.bandwidth().value()?;
    tx.frequency().set(916e6)?;
    tx.sample_rate().set(2e6)?;
    tx.bandwidth().set(1.5e6)?;
    tx.gain().set(20.0)?;
    assert!((tx.frequency().value()? - 916e6).abs() < 1e3);
    assert!((tx.sample_rate().value()? - 2e6).abs() < 1e3);
    assert_eq!(tx.bandwidth().value()?, 1.5e6);
    assert_eq!(tx.gain().value()?, Some(20.0));
    assert!((rx.frequency().value()? - 915e6).abs() < 1e3);
    assert!((rx.sample_rate().value()? - 4e6).abs() < 1e3);
    assert_eq!(rx.bandwidth().value()?, rx_bandwidth);
    assert_eq!(rx.gain().value()?, Some(30.0));
    assert_ne!(rx.gain().elements()?, tx.gain().elements()?);
    assert!(matches!(
        tx.agc().enabled(),
        Err(Error::Unsupported {
            capability: Capability::Agc,
            ..
        })
    ));

    let mut rx_stream = rx.streamer()?;
    let mut tx_stream = tx.streamer()?;
    let mut received = vec![Complex32::default(); 8192];
    let zeros = vec![Complex32::default(); 4096];

    rx_stream.activate()?;
    rx_stream.activate()?;
    let n = rx_stream.read(&mut [&mut received], TIMEOUT_US)?;
    assert!(n > 0 && n <= received.len());
    rx.frequency().set(915e6)?;
    assert!(rx_stream.read(&mut [&mut received], TIMEOUT_US)? > 0);

    tx_stream.activate()?;
    tx_stream.write_all(&[&zeros], None, false, TIMEOUT_US)?;
    tx_stream.deactivate()?;
    rx_stream.deactivate()?;
    rx_stream.deactivate()?;
    assert!(matches!(
        rx_stream.read(&mut [&mut received], 0),
        Err(Error::StreamInactive)
    ));
    drop(tx_stream);
    let mut tx_stream = tx.streamer()?;
    tx_stream.activate()?;
    tx_stream.write_all(&[&zeros], None, false, TIMEOUT_US)?;
    tx_stream.deactivate()?;

    rx_stream.activate()?;
    assert!(rx_stream.read(&mut [&mut received], TIMEOUT_US)? > 0);
    drop(rx_stream);

    let mut recovered = rx.streamer()?;
    recovered.activate()?;
    assert!(recovered.read(&mut [&mut received], TIMEOUT_US)? > 0);
    recovered.deactivate()?;
    Ok(())
}
