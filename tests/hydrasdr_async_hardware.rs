#![cfg(all(
    feature = "hydrasdr",
    any(feature = "smol", feature = "tokio"),
    not(target_arch = "wasm32")
))]

use num_complex::Complex32;
use seify::{AsyncRegistry, AsyncRxStreamer, Error};

const READ_TIMEOUT_US: i64 = 500_000;

async fn read_samples(stream: &mut seify::DynAsyncRxStreamer) -> Result<(), Error> {
    let mut samples = vec![Complex32::default(); 4096];
    let read = stream.read(&mut [&mut samples], READ_TIMEOUT_US).await?;
    assert!(read > 0, "the HydraSDR returned no samples");
    Ok(())
}

async fn exercise_lifecycle() -> Result<(), Error> {
    let registry = AsyncRegistry::default();
    let descriptors = registry.probe("driver=hydrasdr").await?;
    let descriptor = descriptors.first().ok_or(Error::DeviceNotFound)?;
    let device = registry.open(descriptor).await?;
    let rx = device.rx(0).await?;

    // Exercise focused setters before creating a stream.
    assert_eq!(rx.antenna().ports().await?, ["ANT"]);
    assert_eq!(rx.antenna().selected().await?, "ANT");
    let frequency = rx.frequency().value().await?;
    let sample_rate = rx.sample_rate().value().await?;
    rx.frequency().set(frequency).await?;
    rx.sample_rate().set(sample_rate).await?;

    assert!(!rx.agc().enabled().await?);
    assert_eq!(rx.gain().elements().await?, ["LNA", "MIXER", "VGA"]);
    assert_eq!(rx.gain().value().await?, Some(35.0));
    assert_eq!(rx.gain().element("LNA").value().await?, Some(14.0));
    assert_eq!(rx.gain().element("MIXER").value().await?, Some(15.0));
    assert_eq!(rx.gain().element("VGA").value().await?, Some(6.0));
    rx.gain().set(20.0).await?;
    assert_eq!(rx.gain().value().await?, Some(20.0));
    assert_eq!(rx.gain().element("LNA").value().await?, Some(14.0));
    assert_eq!(rx.gain().element("MIXER").value().await?, Some(6.0));
    assert_eq!(rx.gain().element("VGA").value().await?, Some(0.0));

    // Dormant streams also hold an exclusive claim until final close.
    drop(rx.streamer().await?);
    let mut stream = rx.streamer().await?;
    stream.activate().await?;
    read_samples(&mut stream).await?;

    // Device controls remain available while the typed bulk stream is active.
    rx.frequency().set(frequency).await?;
    rx.sample_rate().set(sample_rate).await?;
    rx.gain().set(20.0).await?;
    read_samples(&mut stream).await?;

    // A zero-duration read must remain cancellation-safe whether data has
    // already arrived or the timeout wins the race.
    let mut samples = [Complex32::default(); 64];
    let read = stream.read(&mut [&mut samples], 0).await?;
    assert!(read <= samples.len());

    stream.deactivate().await?;

    // A stopped stream keeps its USB queue for subsequent reactivation.
    stream.activate().await?;
    read_samples(&mut stream).await?;
    stream.deactivate().await?;

    // Stopping retains the claim; dropping must close it before replacement.
    drop(stream);
    let mut stream = rx.streamer().await?;

    // Dropping an active Seify streamer must be recoverable by the next
    // streamer after deferred receiver-off cleanup.
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
#[ignore = "requires an attached HydraSDR"]
fn async_hydrasdr_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "smol")]
    futures::executor::block_on(exercise_lifecycle())?;

    #[cfg(all(not(feature = "smol"), feature = "tokio"))]
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?
        .block_on(exercise_lifecycle())?;

    Ok(())
}
