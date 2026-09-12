use seify::{Driver, Error};

#[test]
fn uhd_driver_aliases() {
    for alias in ["uhd", "usrp", "UHD", "USRP"] {
        assert_eq!(alias.parse::<Driver>().unwrap(), Driver::Uhd);
    }
}

#[test]
#[cfg(not(all(feature = "uhd", not(target_arch = "wasm32"))))]
fn sync_uhd_reports_disabled_feature() {
    assert!(matches!(
        seify::Registry::default().probe("driver=uhd"),
        Err(Error::DriverFeatureNotEnabled {
            driver: Driver::Uhd
        })
    ));
    assert!(matches!(
        seify::DynDevice::from_args("driver=uhd"),
        Err(Error::DriverFeatureNotEnabled {
            driver: Driver::Uhd
        })
    ));
}

#[test]
#[cfg(not(any(
    target_arch = "wasm32",
    all(feature = "uhd", any(feature = "smol", feature = "tokio"))
)))]
fn async_uhd_reports_disabled_feature_or_runtime() {
    futures::executor::block_on(async {
        assert!(matches!(
            seify::AsyncRegistry::default().probe("driver=uhd").await,
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::Uhd
            })
        ));
        assert!(matches!(
            seify::DynAsyncDevice::from_args("driver=uhd").await,
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::Uhd
            })
        ));
    });
}

#[test]
#[cfg(all(feature = "uhd", not(target_arch = "wasm32")))]
fn sync_typed_uhd_rejects_other_drivers_without_opening_hardware() {
    assert!(matches!(
        seify::Device::<seify::impls::Uhd>::from_args("driver=hackrf"),
        Err(Error::DriverMismatch {
            expected: Driver::Uhd,
            requested: Driver::HackRf
        })
    ));
    assert!(matches!(
        seify::Registry::default().probe("driver=uhd,index=bad"),
        Err(Error::InvalidArgument { .. })
    ));
    assert!(matches!(
        seify::DynDevice::from_args("driver=uhd,index=bad"),
        Err(Error::InvalidArgument { .. })
    ));
    assert!(matches!(
        seify::impls::Uhd::open("index=bad"),
        Err(Error::InvalidArgument { .. })
    ));
}

#[test]
#[cfg(all(
    feature = "uhd",
    not(target_arch = "wasm32"),
    any(feature = "smol", feature = "tokio")
))]
fn async_typed_uhd_rejects_other_drivers_without_opening_hardware() {
    futures::executor::block_on(async {
        assert!(matches!(
            seify::AsyncDevice::<seify::impls::AsyncUhd>::from_args("driver=hackrf").await,
            Err(Error::DriverMismatch {
                expected: Driver::Uhd,
                requested: Driver::HackRf
            })
        ));
        assert!(matches!(
            seify::AsyncRegistry::default()
                .probe("driver=uhd,index=bad")
                .await,
            Err(Error::InvalidArgument { .. })
        ));
        assert!(matches!(
            seify::DynAsyncDevice::from_args("driver=uhd,index=bad").await,
            Err(Error::InvalidArgument { .. })
        ));
        assert!(matches!(
            seify::impls::AsyncUhd::open("index=bad").await,
            Err(Error::InvalidArgument { .. })
        ));
    });
}
