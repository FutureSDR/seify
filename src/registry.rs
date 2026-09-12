use serde::{Deserialize, Serialize};

use crate::Args;
use crate::Driver;
use crate::DynDevice;
use crate::Error;

/// Discovered device descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    driver: Driver,
    args: Args,
}

impl DeviceDescriptor {
    /// Create a device descriptor for a driver.
    pub fn new(driver: Driver, args: Args) -> Self {
        Self { driver, args }
    }

    /// Driver that produced this descriptor.
    pub fn driver(&self) -> Driver {
        self.driver
    }

    /// Arguments that identify this device.
    pub fn args(&self) -> &Args {
        &self.args
    }

    /// Consume the descriptor and return its identifying arguments.
    pub fn into_args(self) -> Args {
        self.args
    }
}

/// Typed driver implementation that can be opened directly.
///
/// Cloning an opened backend must create another handle to the same logical
/// device and shared configuration state.
pub trait TypedDeviceBackend: crate::device::DynDeviceBackend + Clone + Sized + 'static {
    /// Driver implemented by this backend.
    fn driver() -> Driver;

    /// Probe devices matching `args`.
    fn probe(args: &Args) -> Result<Vec<Args>, Error>;

    /// Open a typed device matching `args`.
    fn open(args: &Args) -> Result<Self, Error>;
}

struct RegisteredDriver {
    driver: Driver,
    probe: fn(&Args) -> Result<Vec<DeviceDescriptor>, Error>,
    open: fn(&DeviceDescriptor) -> Result<DynDevice, Error>,
}

impl RegisteredDriver {
    fn new<D: TypedDeviceBackend>() -> Self {
        Self {
            driver: <D as TypedDeviceBackend>::driver(),
            probe: probe_typed::<D>,
            open: open_typed::<D>,
        }
    }

    fn driver(&self) -> Driver {
        self.driver
    }

    fn probe(&self, args: &Args) -> Result<Vec<DeviceDescriptor>, Error> {
        (self.probe)(args)
    }

    fn open(&self, descriptor: &DeviceDescriptor) -> Result<DynDevice, Error> {
        (self.open)(descriptor)
    }
}

fn probe_typed<D: TypedDeviceBackend>(args: &Args) -> Result<Vec<DeviceDescriptor>, Error> {
    D::probe(args).map(|descriptors| {
        descriptors
            .into_iter()
            .map(|args| DeviceDescriptor::new(<D as TypedDeviceBackend>::driver(), args))
            .collect()
    })
}

fn open_typed<D: TypedDeviceBackend>(descriptor: &DeviceDescriptor) -> Result<DynDevice, Error> {
    Ok(DynDevice::from_impl(D::open(descriptor.args())?))
}

/// Registry of driver discovery/opening backends.
pub struct Registry {
    backends: Vec<RegisteredDriver>,
}

impl Registry {
    /// Create an empty registry.
    pub fn empty() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Register a typed built-in driver.
    pub fn register<D>(&mut self) -> &mut Self
    where
        D: TypedDeviceBackend,
    {
        self.backends.push(RegisteredDriver::new::<D>());
        self
    }

    pub(crate) fn contains(&self, driver: Driver) -> bool {
        self.backends
            .iter()
            .any(|backend| backend.driver() == driver)
    }

    /// Probe devices matching `args`.
    pub fn probe<A>(&self, args: A) -> Result<Vec<DeviceDescriptor>, Error>
    where
        A: TryInto<Args>,
    {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let driver = requested_driver(&args)?;
        let mut descriptors = Vec::new();
        let mut matched_backend = false;

        for backend in &self.backends {
            if driver.is_none() || driver == Some(backend.driver()) {
                matched_backend = true;
                descriptors.append(&mut backend.probe(&args)?);
            }
        }

        if let Some(driver) = driver {
            if !matched_backend {
                return Err(Error::DriverFeatureNotEnabled { driver });
            }
        }

        Ok(descriptors)
    }

    /// Open a discovered device descriptor.
    pub fn open(&self, descriptor: &DeviceDescriptor) -> Result<DynDevice, Error> {
        let driver = descriptor.driver();
        let mut matched_backend = false;

        for backend in &self.backends {
            if backend.driver() != driver {
                continue;
            }
            matched_backend = true;
            match backend.open(descriptor) {
                Ok(device) => return Ok(device),
                Err(Error::DeviceNotFound) => {}
                Err(e) => return Err(e),
            }
        }

        if !matched_backend {
            return Err(Error::DriverFeatureNotEnabled { driver });
        }

        Err(Error::DeviceNotFound)
    }

    /// Open the first device matching `args`.
    pub fn open_args<A>(&self, args: A) -> Result<DynDevice, Error>
    where
        A: TryInto<Args>,
    {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let driver = requested_driver(&args)?;

        if let Some(driver) = driver {
            let descriptor = DeviceDescriptor::new(driver, args);
            return self.open(&descriptor);
        }

        for backend in &self.backends {
            let descriptor = DeviceDescriptor::new(backend.driver(), args.clone());
            match backend.open(&descriptor) {
                Ok(device) => return Ok(device),
                Err(Error::DeviceNotFound) => {}
                Err(e) => return Err(e),
            }
        }

        Err(Error::DeviceNotFound)
    }
}

impl Default for Registry {
    fn default() -> Self {
        #[allow(unused_mut)]
        let mut registry = Self::empty();

        #[cfg(all(feature = "aaronia_http", not(target_arch = "wasm32")))]
        registry.register::<crate::impls::AaroniaHttp>();

        #[cfg(all(feature = "bladerf1", not(target_arch = "wasm32")))]
        registry.register::<crate::impls::BladeRf>();

        #[cfg(all(feature = "rtlsdr", not(target_arch = "wasm32")))]
        registry.register::<crate::impls::RtlSdr>();

        #[cfg(all(feature = "hackrf", not(target_arch = "wasm32")))]
        registry.register::<crate::impls::HackRf>();

        #[cfg(all(feature = "hydrasdr", not(target_arch = "wasm32")))]
        registry.register::<crate::impls::HydraSdr>();

        #[cfg(all(feature = "uhd", not(target_arch = "wasm32")))]
        registry.register::<crate::impls::Uhd>();

        #[cfg(all(feature = "soapy", not(target_arch = "wasm32")))]
        registry.register::<crate::impls::Soapy>();

        #[cfg(feature = "dummy")]
        registry.register::<crate::impls::Dummy>();

        registry
    }
}

fn requested_driver(args: &Args) -> Result<Option<Driver>, Error> {
    match args.get::<Driver>("driver") {
        Ok(driver) => Ok(Some(driver)),
        Err(Error::MissingArgument { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

#[allow(unused_macros)]
macro_rules! impl_typed_device_backend {
    ($device:ty, $driver:expr) => {
        impl $crate::registry::TypedDeviceBackend for $device {
            fn driver() -> $crate::Driver {
                $driver
            }

            fn probe(args: &$crate::Args) -> Result<Vec<$crate::Args>, $crate::Error> {
                <$device>::probe(args)
            }

            fn open(args: &$crate::Args) -> Result<Self, $crate::Error> {
                <$device>::open(args)
            }
        }
    };
}
#[allow(unused_imports)]
pub(crate) use impl_typed_device_backend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uhd_registration_matches_feature_and_target() {
        assert_eq!(
            Registry::default().contains(Driver::Uhd),
            cfg!(all(feature = "uhd", not(target_arch = "wasm32")))
        );
    }

    #[test]
    fn default_registry_prefers_specific_drivers_before_fallbacks() {
        let drivers: Vec<_> = Registry::default()
            .backends
            .iter()
            .map(|backend| backend.driver())
            .collect();

        let soapy = drivers.iter().position(|driver| *driver == Driver::Soapy);
        let dummy = drivers.iter().position(|driver| *driver == Driver::Dummy);

        for fallback in [soapy, dummy].into_iter().flatten() {
            for (index, driver) in drivers.iter().enumerate() {
                if matches!(driver, Driver::Soapy | Driver::Dummy) {
                    continue;
                }
                assert!(
                    index < fallback,
                    "{driver:?} should be registered before fallback drivers"
                );
            }
        }

        if let (Some(soapy), Some(dummy)) = (soapy, dummy) {
            assert!(soapy < dummy, "dummy should be registered last");
        }
    }

    #[test]
    #[cfg(feature = "dummy")]
    fn default_registry_probes_dummy() {
        let descriptors = Registry::default().probe("driver=dummy").unwrap();

        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].driver(), Driver::Dummy);
        assert_eq!(
            descriptors[0].args().get::<String>("driver").unwrap(),
            "dummy"
        );
    }

    #[test]
    #[cfg(feature = "dummy")]
    fn default_registry_opens_dummy_descriptor() {
        let descriptors = Registry::default().probe("driver=dummy").unwrap();
        let device = Registry::default().open(&descriptors[0]).unwrap();

        assert_eq!(device.driver(), Driver::Dummy);
    }

    #[test]
    #[cfg(feature = "dummy")]
    fn device_from_args_uses_registry() {
        let device = DynDevice::from_args("driver=dummy").unwrap();

        assert_eq!(device.driver(), Driver::Dummy);
    }

    #[test]
    #[cfg(feature = "dummy")]
    fn typed_device_from_args_opens_concrete_backend() {
        let device = crate::Device::<crate::impls::Dummy>::from_args("driver=dummy").unwrap();

        assert_eq!(device.driver(), Driver::Dummy);
    }

    #[test]
    #[cfg(feature = "dummy")]
    fn typed_device_from_args_rejects_mismatched_driver_filter() {
        assert!(matches!(
            crate::Device::<crate::impls::Dummy>::from_args("driver=soapy"),
            Err(Error::DriverMismatch {
                expected: Driver::Dummy,
                requested: Driver::Soapy
            })
        ));
    }

    #[test]
    #[cfg(feature = "dummy")]
    fn enumerate_with_args_uses_registry() {
        let descriptors = crate::enumerate_with_args("driver=dummy").unwrap();

        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].get::<String>("driver").unwrap(), "dummy");
    }

    #[test]
    #[cfg(not(all(feature = "hackrf", not(target_arch = "wasm32"))))]
    fn registry_probe_reports_disabled_hackrf_feature() {
        assert!(matches!(
            Registry::default().probe("driver=hackrf"),
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::HackRf
            })
        ));
    }

    #[test]
    #[cfg(not(all(feature = "hackrf", not(target_arch = "wasm32"))))]
    fn registry_open_args_reports_disabled_hackrf_feature() {
        assert!(matches!(
            Registry::default().open_args("driver=hackrf"),
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::HackRf
            })
        ));
    }

    #[test]
    #[cfg(not(all(feature = "hydrasdr", not(target_arch = "wasm32"))))]
    fn registry_probe_reports_disabled_hydrasdr_feature() {
        assert!(matches!(
            Registry::default().probe("driver=hydrasdr"),
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::HydraSdr
            })
        ));
    }

    #[test]
    #[cfg(not(all(feature = "hydrasdr", not(target_arch = "wasm32"))))]
    fn registry_open_args_reports_disabled_hydrasdr_feature() {
        assert!(matches!(
            Registry::default().open_args("driver=hydrasdr"),
            Err(Error::DriverFeatureNotEnabled {
                driver: Driver::HydraSdr
            })
        ));
    }
}
