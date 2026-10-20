#![cfg(all(feature = "uhd", not(target_arch = "wasm32")))]

use num_complex::Complex32;
use seify::{Args, Device, Error, RxStreamer};

fn args() -> Args {
    let mut args = Args::new();
    args.set("driver", "uhd");
    if let Ok(serial) = std::env::var("UHD_RS_SERIAL") {
        args.set("serial", serial);
    }
    args
}

#[test]
#[ignore = "requires an attached USRP B2xx; run hardware tests serially"]
fn sync_uhd_lifecycle() -> Result<(), Error> {
    let device = Device::<seify::impls::Uhd>::from_args(args())?;
    let rx = device.rx(0)?;
    assert_eq!(rx.antenna().ports()?, ["RX2", "TX/RX"]);
    assert_eq!(rx.gain().elements()?, ["PGA"]);
    rx.gain().set(30.4)?;
    assert_eq!(rx.gain().value()?, Some(30.0));
    rx.agc().enable()?;
    assert_eq!(rx.gain().value()?, None);
    rx.agc().disable()?;
    assert_eq!(rx.gain().value()?, Some(30.0));
    rx.frequency().set(100_000_000.0)?;
    assert!((rx.frequency().value()? - 100_000_000.0).abs() < 1.0);
    rx.sample_rate().set(1_100_000.0)?;
    assert!((rx.sample_rate().value()? - 1_100_000.0).abs() < 1.0);

    let dynamic = device.to_dyn();
    let capabilities = dynamic.capabilities()?;
    assert!(capabilities.rx_channels[0]
        .controls
        .bandwidth_range
        .is_some());
    assert!(capabilities.rx_channels[0].controls.dc_offset);
    let channel = dynamic.rx(0)?;
    assert!(channel.sample_rate().range()?.contains(429.6875));
    assert!(channel.sample_rate().range()?.contains(61.44e6));
    assert!(channel.sample_rate().range()?.contains(17e6));
    for antenna in ["TX/RX", "RX2"] {
        channel.antenna().select(antenna)?;
        assert_eq!(rx.antenna().selected()?, antenna);
    }
    channel.bandwidth().set(2e6)?;
    channel.dc_offset().disable()?;
    for rate in [429.6875, 1e3, 31_249.0, 1.1e6, 7e6, 17e6, 30.72e6, 61.44e6] {
        channel.sample_rate().set(rate)?;
        assert!((channel.sample_rate().value()? - rate).abs() < 1.0);
        assert!((channel.bandwidth().value()? - (rate * 512.0).min(2e6)).abs() < 1.0);
    }
    assert!(!rx.dc_offset().enabled()?);
    channel.sample_rate().set(7e6)?;
    channel.bandwidth().set(50e6)?;
    channel.sample_rate().set(16e6)?;
    assert_eq!(rx.bandwidth().value()?, 16e6);
    channel.sample_rate().set(7e6)?;
    assert_eq!(rx.bandwidth().value()?, 50e6);
    channel.bandwidth().set(0.0)?;
    assert_eq!(rx.bandwidth().value()?, 56e6);
    channel.sample_rate().set(61.44e6)?;
    channel
        .frequency()
        .set_with_args(100e6, Args::from("OFFSET=20000000")?)?;
    assert!((channel.frequency().value()? - 100e6).abs() < 1.0);
    channel.frequency().set(100e6)?;
    channel.dc_offset().enable()?;
    assert!(rx.dc_offset().enabled()?);

    rx.sample_rate().set(20_000_000.0)?;
    assert_eq!(rx.sample_rate().value()?, 20_000_000.0);
    assert_eq!(rx.gain().value()?, Some(30.0));
    rx.streamer()?.close()?;
    let mut stream = rx.streamer()?;
    assert!(matches!(rx.streamer(), Err(Error::Busy)));
    assert!(matches!(device.as_inner().shutdown(), Err(Error::Busy)));
    assert_eq!(rx.gain().value()?, Some(30.0));
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
    assert!(stream.read(&mut [], 0).is_err());
    assert_eq!(stream.read(&mut [&mut samples], 2_000_000)?, 1);
    stream.deactivate()?;
    stream.activate()?;
    assert_eq!(stream.read(&mut [&mut samples], 2_000_000)?, 1);
    stream.close()?;
    let mut stream = rx.streamer()?;
    // Stream ownership keeps the hardware alive after the device is dropped.
    drop(device);
    stream.activate()?;
    assert_eq!(stream.read(&mut [&mut samples], 2_000_000)?, 1);
    stream.close()?;
    Ok(())
}

#[cfg(any(feature = "smol", feature = "tokio"))]
async fn async_lifecycle() -> Result<(), Error> {
    use seify::{AsyncDevice, AsyncRxStreamer};
    let device = AsyncDevice::<seify::impls::AsyncUhd>::from_args(args()).await?;
    let rx = device.rx(0).await?;
    assert_eq!(rx.antenna().ports().await?, ["RX2", "TX/RX"]);
    rx.gain().set(30.4).await?;
    assert_eq!(rx.gain().value().await?, Some(30.0));
    rx.agc().enable().await?;
    assert_eq!(rx.gain().value().await?, None);
    rx.agc().disable().await?;
    rx.frequency().set(100_000_000.0).await?;
    rx.sample_rate().set(1_100_000.0).await?;
    assert!((rx.sample_rate().value().await? - 1_100_000.0).abs() < 1.0);

    let dynamic = device.to_dyn();
    let capabilities = dynamic.capabilities().await?;
    assert!(capabilities.rx_channels[0]
        .controls
        .bandwidth_range
        .is_some());
    assert!(capabilities.rx_channels[0].controls.dc_offset);
    let channel = dynamic.rx(0).await?;
    assert!(channel.sample_rate().range().await?.contains(429.6875));
    assert!(channel.sample_rate().range().await?.contains(61.44e6));
    assert!(channel.sample_rate().range().await?.contains(17e6));
    for antenna in ["TX/RX", "RX2"] {
        channel.antenna().select(antenna).await?;
        assert_eq!(rx.antenna().selected().await?, antenna);
    }
    channel.bandwidth().set(2e6).await?;
    channel.dc_offset().disable().await?;
    for rate in [429.6875, 1e3, 31_249.0, 1.1e6, 7e6, 17e6, 30.72e6, 61.44e6] {
        channel.sample_rate().set(rate).await?;
        assert!((channel.sample_rate().value().await? - rate).abs() < 1.0);
        assert!((channel.bandwidth().value().await? - (rate * 512.0).min(2e6)).abs() < 1.0);
    }
    assert!(!rx.dc_offset().enabled().await?);
    channel.sample_rate().set(7e6).await?;
    channel.bandwidth().set(50e6).await?;
    channel.sample_rate().set(16e6).await?;
    assert_eq!(rx.bandwidth().value().await?, 16e6);
    channel.sample_rate().set(7e6).await?;
    assert_eq!(rx.bandwidth().value().await?, 50e6);
    channel.bandwidth().set(0.0).await?;
    assert_eq!(rx.bandwidth().value().await?, 56e6);
    channel.sample_rate().set(61.44e6).await?;
    channel
        .frequency()
        .set_with_args(100e6, Args::from("OFFSET=20000000")?)
        .await?;
    assert!((channel.frequency().value().await? - 100e6).abs() < 1.0);
    channel.frequency().set(100e6).await?;
    channel.dc_offset().enable().await?;
    assert!(rx.dc_offset().enabled().await?);

    rx.sample_rate().set(20_000_000.0).await?;
    assert_eq!(rx.sample_rate().value().await?, 20_000_000.0);
    assert_eq!(rx.gain().value().await?, Some(30.0));
    rx.streamer().await?.close().await?;
    let mut stream = rx.streamer().await?;
    assert!(matches!(rx.streamer().await, Err(Error::Busy)));
    assert!(matches!(
        device.as_inner().shutdown().await,
        Err(Error::Busy)
    ));
    assert_eq!(rx.gain().value().await?, Some(30.0));
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
    assert!(stream.read(&mut [], 0).await.is_err());
    assert_eq!(stream.read(&mut [&mut samples], 2_000_000).await?, 1);
    stream.deactivate().await?;
    stream.activate().await?;
    assert_eq!(stream.read(&mut [&mut samples], 2_000_000).await?, 1);
    stream.close().await?;
    let mut stream = rx.streamer().await?;
    drop(device);
    stream.activate().await?;
    assert_eq!(stream.read(&mut [&mut samples], 2_000_000).await?, 1);
    stream.close().await?;
    Ok(())
}

#[test]
#[cfg(any(feature = "smol", feature = "tokio"))]
#[ignore = "requires an attached USRP B2xx; run hardware tests serially"]
fn async_uhd_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "smol")]
    futures::executor::block_on(async_lifecycle())?;
    #[cfg(all(not(feature = "smol"), feature = "tokio"))]
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?
        .block_on(async_lifecycle())?;
    Ok(())
}

#[test]
#[ignore = "requires an attached USRP B2xx; run hardware tests serially"]
fn sync_uhd_receive_across_rf_bands() -> Result<(), Error> {
    let device = Device::<seify::impls::Uhd>::from_args(args())?;
    println!("Motherboard: {:?}", device.info()?);
    let rx = device.rx(0)?;
    rx.sample_rate().set(1_000_000.0)?;
    let mut stream = rx.streamer()?;
    let mut samples = vec![Complex32::default(); stream.mtu()?];
    for frequency in [100_000_000.0, 3_000_000_000.0, 5_000_000_000.0] {
        rx.frequency().set(frequency)?;
        assert!((rx.frequency().value()? - frequency).abs() < 1.0);
        for gain in [10.0, 50.0] {
            rx.gain().set(gain)?;
            stream.activate()?;
            let mut count = 0;
            let mut energy = 0.0_f64;
            let mut peak = 0.0_f64;
            let mut first = None;
            let mut varied = false;
            while count < 65_536 {
                let n = stream.read(&mut [&mut samples], 2_000_000)?;
                assert!(n > 0);
                for &sample in &samples[..n] {
                    assert!(sample.re.is_finite() && sample.im.is_finite());
                    let power = f64::from(sample.norm_sqr());
                    energy += power;
                    peak = peak.max(power);
                    varied |= sample != *first.get_or_insert(sample);
                }
                count += n;
            }
            stream.deactivate()?;
            println!(
                "{frequency} Hz, {gain} dB: {count} samples, RMS={}, peak={}",
                (energy / count as f64).sqrt(),
                peak.sqrt()
            );
            assert!(
                energy > 0.0 && varied,
                "RX returned only zero or constant samples"
            );
        }
    }
    let stats = stream.close()?;
    println!("Stream statistics: {stats:?}");
    device.as_inner().shutdown()?;
    Ok(())
}
