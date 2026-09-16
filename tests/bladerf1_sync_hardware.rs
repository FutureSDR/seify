#![cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]

use num_complex::Complex32;
use seify::{Error, Registry, RxStreamer, TxStreamer};

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
    rx.gain().set(30.0)?;
    assert!((rx.gain().value()?.unwrap() - 30.0).abs() <= 3.0);
    assert!(!rx.gain().elements()?.is_empty());

    let mut rx_stream = rx.streamer()?;
    let mut tx_stream = tx.streamer()?;
    let mut received = vec![Complex32::default(); 8192];
    let zeros = vec![Complex32::default(); 4096];

    rx_stream.activate()?;
    let n = rx_stream.read(&mut [&mut received], TIMEOUT_US)?;
    assert!(n > 0 && n <= received.len());
    rx.frequency().set(915e6)?;
    assert!(rx_stream.read(&mut [&mut received], TIMEOUT_US)? > 0);

    tx_stream.activate()?;
    tx_stream.write_all(&[&zeros], None, false, TIMEOUT_US)?;
    tx_stream.deactivate()?;
    rx_stream.deactivate()?;

    rx_stream.activate()?;
    assert!(rx_stream.read(&mut [&mut received], TIMEOUT_US)? > 0);
    drop(rx_stream);

    let mut recovered = rx.streamer()?;
    recovered.activate()?;
    assert!(recovered.read(&mut [&mut received], TIMEOUT_US)? > 0);
    recovered.deactivate()?;
    Ok(())
}
