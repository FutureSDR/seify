#![cfg(all(
    feature = "bladerf1",
    any(feature = "smol", feature = "tokio"),
    not(target_arch = "wasm32")
))]

use num_complex::Complex32;
use seify::{AsyncRegistry, AsyncRxStreamer, AsyncTxStreamer, Error};

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
    rx.gain().set(30.0).await?;
    assert!((rx.gain().value().await?.unwrap() - 30.0).abs() <= 3.0);
    assert!(!rx.gain().elements().await?.is_empty());
    assert!(rx.bandwidth().value().await? > 0.0);

    let mut stream = rx.streamer().await?;
    assert!(matches!(
        stream.read(&mut [&mut [Complex32::default(); 16]], 0).await,
        Err(Error::StreamInactive)
    ));
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
    tx_stream.deactivate().await?;
    drop(tx_stream);

    stream.deactivate().await?;

    // A stopped stream keeps its USB queue for subsequent reactivation.
    stream.activate().await?;
    read_samples(&mut stream).await?;
    stream.deactivate().await?;

    // Dropping an active streamer must be recoverable by the next streamer
    // after deferred teardown.
    stream.activate().await?;
    read_samples(&mut stream).await?;
    drop(stream);

    let mut recovered = rx.streamer().await?;
    recovered.activate().await?;
    read_samples(&mut recovered).await?;
    recovered.deactivate().await?;

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
