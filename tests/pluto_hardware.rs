#![cfg(all(feature = "pluto", not(target_arch = "wasm32")))]

#[cfg(any(feature = "smol", feature = "tokio"))]
use seify::Args;
use seify::{impls::Pluto, Device, Direction, Driver, Error, Registry};

// Keep all Pluto hardware work in one test to avoid competing USB claims.
#[test]
#[ignore = "requires a connected PlutoSDR with USB access permissions"]
fn pluto_registry_and_shared_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let registry = Registry::default();
    let descriptors = registry.probe("driver=pluto")?;
    let descriptor = descriptors.first().ok_or(Error::DeviceNotFound)?;
    assert_eq!(descriptor.driver(), Driver::Pluto);
    let args = descriptor.args().clone();
    let device = Device::<Pluto>::from_args(args.clone())?;
    let clone = device.clone();
    let dynamic = device.to_dyn();
    let serial = args.get::<String>("serial")?;
    assert_eq!(device.id()?, serial);
    assert_eq!(dynamic.id()?, serial);
    assert_eq!(device.num_channels(Direction::Rx)?, 0);
    assert_eq!(device.num_channels(Direction::Tx)?, 0);
    let capabilities = dynamic.capabilities()?;
    assert!(capabilities.rx_channels.is_empty());
    assert!(capabilities.tx_channels.is_empty());
    assert!(!capabilities.full_duplex);
    assert!(matches!(dynamic.rx(0), Err(Error::InvalidChannel { .. })));
    assert!(matches!(
        dynamic.rx_streamer(&[]),
        Err(Error::Unsupported { .. })
    ));
    assert!(matches!(
        dynamic.tx_streamer(&[]),
        Err(Error::Unsupported { .. })
    ));
    assert_eq!(dynamic.info()?.get::<String>("transport")?, "usb");
    let context = device.as_inner().context();
    assert!(context
        .devices
        .iter()
        .any(|d| d.name.as_deref() == Some("ad9361-phy")));
    assert_eq!(
        device.info()?.get::<usize>("iio_device_count")?,
        context.devices.len()
    );
    let info = device.info()?;
    drop(device);
    assert_eq!(clone.info()?, info);
    clone.as_inner().shutdown()?;
    clone.as_inner().shutdown()?;
    // Shutdown must release USB even with other typed and dynamic handles alive.
    let reopened = registry.open(descriptor)?;
    assert_eq!(reopened.id()?, serial);
    drop(reopened);
    let reopened = Device::<Pluto>::from_args(args.clone())?;
    reopened.as_inner().shutdown()?;
    eprintln!(
        "Pluto Seify sync: registry, context, capability limits, clone/shutdown/reopen passed"
    );

    #[cfg(feature = "smol")]
    futures::executor::block_on(async_lifecycle(args))?;
    #[cfg(all(not(feature = "smol"), feature = "tokio"))]
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?
        .block_on(async_lifecycle(args))?;
    Ok(())
}

#[cfg(any(feature = "smol", feature = "tokio"))]
async fn async_lifecycle(args: Args) -> Result<(), Error> {
    use seify::{impls::AsyncPluto, AsyncDevice, AsyncRegistry};
    let registry = AsyncRegistry::default();
    let descriptors = registry.probe(args.clone()).await?;
    assert_eq!(descriptors.len(), 1);
    let device = AsyncDevice::<AsyncPluto>::from_args(args.clone()).await?;
    let clone = device.clone();
    let dynamic = device.to_dyn();
    assert_eq!(dynamic.id().await?, args.get::<String>("serial")?);
    assert_eq!(device.info().await?, dynamic.info().await?);
    assert!(dynamic.capabilities().await?.rx_channels.is_empty());
    assert!(dynamic.capabilities().await?.tx_channels.is_empty());
    assert!(matches!(
        dynamic.rx_streamer(&[]).await,
        Err(Error::Unsupported { .. })
    ));
    assert!(matches!(
        dynamic.tx_streamer(&[]).await,
        Err(Error::Unsupported { .. })
    ));
    assert!(!clone.as_inner().context().devices.is_empty());
    drop(device);
    clone.as_inner().shutdown().await?;
    clone.as_inner().shutdown().await?;
    let reopened = registry.open(&descriptors[0]).await?;
    assert_eq!(reopened.id().await?, args.get::<String>("serial")?);
    drop(reopened);
    let reopened = AsyncDevice::<AsyncPluto>::from_args(args).await?;
    reopened.as_inner().shutdown().await?;
    eprintln!(
        "Pluto Seify async: registry, context, capability limits, clone/shutdown/reopen passed"
    );
    Ok(())
}
