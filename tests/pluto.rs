use seify::{Driver, Error};

#[test]
fn pluto_aliases() {
    for alias in ["pluto", "plutosdr", "adalm-pluto", "PLUTO", "PlutoSDR"] {
        assert_eq!(alias.parse::<Driver>().unwrap(), Driver::Pluto);
    }
}

#[test]
#[cfg(not(all(feature = "pluto", not(target_arch = "wasm32"))))]
fn sync_pluto_reports_disabled_feature() {
    assert!(matches!(
        seify::Registry::default().probe("driver=pluto"),
        Err(Error::DriverFeatureNotEnabled {
            driver: Driver::Pluto
        })
    ));
    assert!(matches!(
        seify::DynDevice::from_args("driver=pluto"),
        Err(Error::DriverFeatureNotEnabled {
            driver: Driver::Pluto
        })
    ));
}

#[test]
#[cfg(all(
    not(target_arch = "wasm32"),
    not(all(feature = "pluto", any(feature = "smol", feature = "tokio")))
))]
fn async_pluto_reports_disabled_feature_or_runtime() {
    futures::executor::block_on(async {
        assert!(matches!(
            seify::AsyncRegistry::default().probe("driver=pluto").await,
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::Pluto
            })
        ));
        assert!(matches!(
            seify::DynAsyncDevice::from_args("driver=pluto").await,
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::Pluto
            })
        ));
    });
}

#[test]
#[cfg(all(feature = "pluto", not(target_arch = "wasm32")))]
fn sync_invalid_arguments_fail_without_usb() {
    assert!(matches!(
        seify::Device::<seify::impls::Pluto>::from_args("driver=hackrf"),
        Err(Error::DriverMismatch {
            expected: Driver::Pluto,
            requested: Driver::HackRf
        })
    ));
    assert!(matches!(
        seify::Registry::default().probe("driver=pluto,index=bad"),
        Err(Error::InvalidArgument { .. })
    ));
    assert!(matches!(
        seify::DynDevice::from_args("driver=pluto,index=bad"),
        Err(Error::InvalidArgument { .. })
    ));
    assert!(matches!(
        seify::impls::Pluto::open("index=bad"),
        Err(Error::InvalidArgument { .. })
    ));
    fn send_sync<T: Send + Sync>() {}
    send_sync::<seify::impls::Pluto>();
    fn capabilities<
        T: seify::RxDevice
            + seify::GainControl
            + seify::DcOffsetControl
            + seify::AgcControl
            + seify::FrequencyControl
            + seify::SampleRateControl
            + seify::BandwidthControl
            + seify::AntennaControl,
    >() {
    }
    capabilities::<seify::impls::Pluto>();
}

#[test]
#[cfg(all(
    feature = "pluto",
    not(target_arch = "wasm32"),
    any(feature = "smol", feature = "tokio")
))]
fn async_invalid_arguments_fail_without_usb_and_futures_are_send() {
    use seify::impls::AsyncPluto;
    fn send<T: Send>(_: T) {}
    fn send_sync<T: Send + Sync>() {}
    send_sync::<AsyncPluto>();
    fn capabilities<
        T: seify::AsyncRxDevice
            + seify::AsyncGainControl
            + seify::AsyncDcOffsetControl
            + seify::AsyncAgcControl
            + seify::AsyncFrequencyControl
            + seify::AsyncSampleRateControl
            + seify::AsyncBandwidthControl
            + seify::AsyncAntennaControl,
    >() {
    }
    capabilities::<AsyncPluto>();
    send(AsyncPluto::open("driver=pluto"));
    futures::executor::block_on(async {
        assert!(matches!(
            seify::AsyncDevice::<AsyncPluto>::from_args("driver=hackrf").await,
            Err(Error::DriverMismatch {
                expected: Driver::Pluto,
                requested: Driver::HackRf
            })
        ));
        assert!(matches!(
            seify::AsyncRegistry::default()
                .probe("driver=pluto,index=bad")
                .await,
            Err(Error::InvalidArgument { .. })
        ));
        assert!(matches!(
            seify::DynAsyncDevice::from_args("driver=pluto,index=bad").await,
            Err(Error::InvalidArgument { .. })
        ));
        assert!(matches!(
            AsyncPluto::open("index=bad").await,
            Err(Error::InvalidArgument { .. })
        ));
    });
}

#[cfg(all(feature = "pluto", target_arch = "wasm32"))]
#[test]
fn webusb_filter_uses_exact_serial_and_standard_pluto_identity() {
    use seify::{
        dev::{AsyncTypedDeviceBackend, WebUsbDeviceFilter},
        impls::AsyncPluto,
        Args,
    };
    let filter =
        AsyncPluto::webusb_filters(&Args::from("driver=pluto,serial=000AbC").unwrap()).unwrap();
    assert_eq!(
        filter,
        vec![WebUsbDeviceFilter::new()
            .with_vendor_product(0x0456, 0xb673)
            .with_serial_number("000AbC")]
    );
}
