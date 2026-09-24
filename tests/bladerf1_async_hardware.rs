#![cfg(all(
    feature = "bladerf1",
    any(feature = "smol", feature = "tokio"),
    not(target_arch = "wasm32")
))]

use futures::FutureExt;
use num_complex::Complex32;
use seify::{AsyncRegistry, AsyncRxStreamer, AsyncTxStreamer, Capability, Error};

const TIMEOUT_US: i64 = 2_000_000;

async fn read_samples(stream: &mut seify::DynAsyncRxStreamer) -> Result<(), Error> {
    let mut samples = vec![Complex32::default(); 8192];
    let read = stream.read(&mut [&mut samples], TIMEOUT_US).await?;
    assert!(read > 0, "the bladeRF returned no samples");
    assert!(read <= samples.len());
    Ok(())
}

async fn exercise_lifecycle() -> Result<(), Error> {
    let registry = AsyncRegistry::default();
    let descriptors = registry.probe("driver=bladerf").await?;
    let descriptor = descriptors.first().ok_or(Error::DeviceNotFound)?;
    let device = registry.open(descriptor).await?;
    assert_eq!(
        device.id().await?,
        descriptor.args().get::<String>("serial")?
    );
    let rx = device.rx(0).await?;
    let tx = device.tx(0).await?;

    rx.frequency().set(915e6).await?;
    assert!((rx.frequency().value().await? - 915e6).abs() < 1e3);
    rx.sample_rate().set(4e6).await?;
    assert!((rx.sample_rate().value().await? - 4e6).abs() < 1e3);
    rx.agc().disable().await?;
    assert!(!rx.agc().enabled().await?);
    rx.gain().set(30.0).await?;
    assert!((rx.gain().value().await?.unwrap() - 30.0).abs() <= 3.0);
    assert!(!rx.gain().elements().await?.is_empty());
    assert!(rx.bandwidth().value().await? > 0.0);
    let rx_bandwidth = rx.bandwidth().value().await?;
    tx.frequency().set(916e6).await?;
    tx.sample_rate().set(2e6).await?;
    tx.bandwidth().set(1.5e6).await?;
    tx.gain().set(20.0).await?;
    assert!((tx.frequency().value().await? - 916e6).abs() < 1e3);
    assert!((tx.sample_rate().value().await? - 2e6).abs() < 1e3);
    assert_eq!(tx.bandwidth().value().await?, 1.5e6);
    assert_eq!(tx.gain().value().await?, Some(20.0));
    assert!((rx.frequency().value().await? - 915e6).abs() < 1e3);
    assert!((rx.sample_rate().value().await? - 4e6).abs() < 1e3);
    assert_eq!(rx.bandwidth().value().await?, rx_bandwidth);
    assert_eq!(rx.gain().value().await?, Some(30.0));
    assert_ne!(rx.gain().elements().await?, tx.gain().elements().await?);
    assert!(matches!(
        tx.agc().enabled().await,
        Err(Error::Unsupported {
            capability: Capability::Agc,
            ..
        })
    ));

    let mut stream = rx.streamer().await?;
    assert!(matches!(
        stream.read(&mut [&mut [Complex32::default(); 16]], 0).await,
        Err(Error::StreamInactive)
    ));
    if let Some(result) = stream.activate().now_or_never() {
        result?;
    }
    stream.deactivate().await?;
    stream.activate().await?;
    stream.activate().await?;
    read_samples(&mut stream).await?;

    // Device controls remain available while the stream is active.
    rx.frequency().set(915e6).await?;
    rx.gain().set(20.0).await?;
    read_samples(&mut stream).await?;

    // A zero-duration read must be cancellation-safe whether data has
    // already arrived or the timeout wins the race.
    let mut samples = [Complex32::default(); 64];
    let read = stream.read(&mut [&mut samples], 0).await?;
    assert!(read <= samples.len());
    read_samples(&mut stream).await?;

    // TX round trip while RX is active (full duplex).
    let zeros = vec![Complex32::default(); 4096];
    let mut tx_stream = tx.streamer().await?;
    tx_stream.activate().await?;
    tx_stream
        .write_all(&[&zeros], None, false, TIMEOUT_US)
        .await?;
    if let Some(result) = tx_stream.deactivate().now_or_never() {
        result?;
    }
    tx_stream.deactivate().await?;
    drop(tx_stream);

    if let Some(result) = stream.deactivate().now_or_never() {
        result?;
    }
    stream.deactivate().await?;
    stream.deactivate().await?;
    assert!(matches!(
        stream.read(&mut [&mut samples], 0).await,
        Err(Error::StreamInactive)
    ));

    // A stopped stream keeps its USB queue for subsequent reactivation.
    stream.activate().await?;
    read_samples(&mut stream).await?;
    stream.deactivate().await?;

    // Dropping an active streamer must be recoverable by the next streamer
    // after deferred teardown.
    stream.activate().await?;
    read_samples(&mut stream).await?;
    drop(stream);

    if let Some(result) = device.info().now_or_never() {
        result?;
    }
    device.info().await?;

    let mut recovered = rx.streamer().await?;
    recovered.activate().await?;
    read_samples(&mut recovered).await?;
    recovered.deactivate().await?;
    drop(recovered);
    device.info().await?;

    Ok(())
}

#[test]
#[ignore = "requires an attached bladeRF 1"]
fn async_bladerf_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "smol")]
    futures::executor::block_on(exercise_lifecycle())?;

    #[cfg(all(not(feature = "smol"), feature = "tokio"))]
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?
        .block_on(exercise_lifecycle())?;

    Ok(())
}
