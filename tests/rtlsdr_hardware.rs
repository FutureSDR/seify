#![cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]

use num_complex::Complex32;
use seify::{Error, Registry, RxStreamer};

const READ_TIMEOUT_US: i64 = 1_000_000;

#[test]
#[ignore = "requires an attached RTL-SDR with a supported tuner"]
fn sync_rtlsdr_lifecycle() -> Result<(), Error> {
    let device = Registry::default().open_args("driver=rtlsdr")?;
    let rx = device.rx(0)?;
    assert_eq!(rx.antenna().ports()?, ["RX"]);
    assert_eq!(rx.gain().elements()?, ["TUNER"]);
    rx.gain().set(23.5)?;
    assert_eq!(rx.gain().value()?, Some(23.5));
    assert!(!rx.agc().enabled()?);
    rx.agc().enable()?;
    assert_eq!(rx.gain().value()?, None);
    rx.frequency().set(100_000_000.0)?;
    rx.sample_rate().set(2_048_000.0)?;

    drop(rx.streamer()?);
    let mut stream = rx.streamer()?;
    assert!(matches!(rx.streamer(), Err(Error::Busy)));
    let mut samples = [Complex32::default(); 1];
    assert!(matches!(
        stream.read(&mut [&mut samples], 0),
        Err(Error::StreamInactive)
    ));
    assert!(matches!(
        stream.activate_at(Some(0)),
        Err(Error::Unsupported { .. })
    ));
    stream.activate()?;
    // Single-sample buffers no longer get rounded down to an empty transfer.
    assert_eq!(stream.read(&mut [&mut samples], READ_TIMEOUT_US)?, 1);
    stream.deactivate()?;
    drop(stream);
    let mut stream = rx.streamer()?;
    stream.activate()?;
    drop(stream);
    let mut stream = rx.streamer()?;

    // The owned stream keeps its hardware alive after all device handles drop.
    drop(device);
    stream.activate()?;
    assert_eq!(stream.read(&mut [&mut samples], READ_TIMEOUT_US)?, 1);
    stream.deactivate()?;
    stream.activate()?;
    assert_eq!(stream.read(&mut [&mut samples], READ_TIMEOUT_US)?, 1);
    stream.deactivate()?;
    Ok(())
}

#[cfg(any(feature = "smol", feature = "tokio"))]
async fn async_lifecycle() -> Result<(), Error> {
    use seify::{AsyncRegistry, AsyncRxStreamer};

    let device = AsyncRegistry::default().open_args("driver=rtlsdr").await?;
    let rx = device.rx(0).await?;
    assert_eq!(rx.antenna().ports().await?, ["RX"]);
    assert_eq!(rx.gain().elements().await?, ["TUNER"]);
    rx.gain().set(23.5).await?;
    assert_eq!(rx.gain().value().await?, Some(23.5));
    assert!(!rx.agc().enabled().await?);
    rx.frequency().set(100_000_000.0).await?;
    rx.sample_rate().set(2_048_000.0).await?;

    drop(rx.streamer().await?);
    let mut stream = rx.streamer().await?;
    assert!(matches!(rx.streamer().await, Err(Error::Busy)));
    let mut samples = [Complex32::default(); 1];
    assert!(matches!(
        stream.read(&mut [&mut samples], 0).await,
        Err(Error::StreamInactive)
    ));
    assert!(matches!(
        stream.activate_at(Some(0)).await,
        Err(Error::Unsupported { .. })
    ));
    stream.activate().await?;
    assert_eq!(stream.read(&mut [&mut samples], READ_TIMEOUT_US).await?, 1);
    assert!(stream.read(&mut [&mut samples], 0).await? <= 1);
    stream.deactivate().await?;
    stream.activate().await?;
    assert_eq!(stream.read(&mut [&mut samples], READ_TIMEOUT_US).await?, 1);
    stream.deactivate().await?;
    drop(stream);
    let mut stream = rx.streamer().await?;
    stream.activate().await?;
    drop(stream);
    let mut stream = rx.streamer().await?;

    drop(device);
    stream.activate().await?;
    assert_eq!(stream.read(&mut [&mut samples], READ_TIMEOUT_US).await?, 1);
    stream.deactivate().await?;
    Ok(())
}

#[test]
#[cfg(any(feature = "smol", feature = "tokio"))]
#[ignore = "requires an attached RTL-SDR with a supported tuner"]
fn async_rtlsdr_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "smol")]
    futures::executor::block_on(async_lifecycle())?;
    #[cfg(all(not(feature = "smol"), feature = "tokio"))]
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?
        .block_on(async_lifecycle())?;
    Ok(())
}
