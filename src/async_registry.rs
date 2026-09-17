use std::future::Future;

use crate::async_device::{AsyncDeviceInfo, DynAsyncDeviceBackend};
use crate::AsyncFutureExt;
use crate::{
    Args, BoxedFuture, Capability, DeviceDescriptor, Driver, DynAsyncDevice, Error, MaybeSend,
};

/// Typed asynchronous driver implementation that can be opened directly.
///
/// Implementations may use `async fn` for `async_probe` and `async_open`; the
/// explicit trait return type enforces Seify's target-dependent `MaybeSend`
/// future bound. Cloning an opened backend must create another handle to the
/// same logical device and shared configuration state.
pub trait AsyncTypedDeviceBackend:
    AsyncDeviceInfo + DynAsyncDeviceBackend + Clone + Sized + 'static
{
    /// Driver implemented by this backend.
    fn driver() -> Driver;
    /// Return browser chooser filters for devices supported by this backend.
    ///
    /// The registry combines filters from every matching backend into one WebUSB
    /// permission request. An empty list means the backend uses no WebUSB device.
    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(_args: &Args) -> Result<Vec<WebUsbDeviceFilter>, Error> {
        Ok(Vec::new())
    }
    /// Probe devices matching `args`.
    fn async_probe<'a>(
        args: &'a Args,
    ) -> impl Future<Output = Result<Vec<Args>, Error>> + MaybeSend + 'a;
    /// Open a typed device matching `args`.
    fn async_open<'a>(args: &'a Args)
        -> impl Future<Output = Result<Self, Error>> + MaybeSend + 'a;
}

/// WebUSB device chooser filter contributed by an asynchronous backend.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebUsbDeviceFilter {
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    class_code: Option<u8>,
    subclass_code: Option<u8>,
    protocol_code: Option<u8>,
    serial_number: Option<String>,
}

#[cfg(target_arch = "wasm32")]
impl WebUsbDeviceFilter {
    /// Create a filter that initially matches every WebUSB device.
    pub const fn new() -> Self {
        Self {
            vendor_id: None,
            product_id: None,
            class_code: None,
            subclass_code: None,
            protocol_code: None,
            serial_number: None,
        }
    }

    /// Restrict the filter to a USB vendor ID.
    pub const fn with_vendor_id(mut self, vendor_id: u16) -> Self {
        self.vendor_id = Some(vendor_id);
        self
    }

    /// Restrict the filter to a USB vendor and product ID.
    pub const fn with_vendor_product(mut self, vendor_id: u16, product_id: u16) -> Self {
        self.vendor_id = Some(vendor_id);
        self.product_id = Some(product_id);
        self
    }

    /// Restrict the filter to a USB device class.
    pub const fn with_class(mut self, class_code: u8) -> Self {
        self.class_code = Some(class_code);
        self
    }

    /// Restrict the filter to a USB device class and subclass.
    pub const fn with_class_subclass(mut self, class_code: u8, subclass_code: u8) -> Self {
        self.class_code = Some(class_code);
        self.subclass_code = Some(subclass_code);
        self
    }

    /// Restrict the filter to a USB device class, subclass, and protocol.
    pub const fn with_class_subclass_protocol(
        mut self,
        class_code: u8,
        subclass_code: u8,
        protocol_code: u8,
    ) -> Self {
        self.class_code = Some(class_code);
        self.subclass_code = Some(subclass_code);
        self.protocol_code = Some(protocol_code);
        self
    }

    /// Restrict the filter to an exact USB serial-number string.
    pub fn with_serial_number(mut self, serial_number: impl Into<String>) -> Self {
        self.serial_number = Some(serial_number.into());
        self
    }

    fn to_web_sys(&self) -> web_sys::UsbDeviceFilter {
        let filter = web_sys::UsbDeviceFilter::new();
        if let Some(vendor_id) = self.vendor_id {
            filter.set_vendor_id(vendor_id);
        }
        if let Some(product_id) = self.product_id {
            filter.set_product_id(product_id);
        }
        if let Some(class_code) = self.class_code {
            filter.set_class_code(class_code);
        }
        if let Some(subclass_code) = self.subclass_code {
            filter.set_subclass_code(subclass_code);
        }
        if let Some(protocol_code) = self.protocol_code {
            filter.set_protocol_code(protocol_code);
        }
        if let Some(serial_number) = &self.serial_number {
            filter.set_serial_number(serial_number);
        }
        filter
    }
}

type ProbeFuture<'a> = BoxedFuture<'a, Result<Vec<DeviceDescriptor>, Error>>;
type OpenFuture<'a> = BoxedFuture<'a, Result<DynAsyncDevice, Error>>;

struct RegisteredAsyncDriver {
    driver: Driver,
    #[cfg(target_arch = "wasm32")]
    webusb_filters: fn(&Args) -> Result<Vec<WebUsbDeviceFilter>, Error>,
    probe: for<'a> fn(&'a Args) -> ProbeFuture<'a>,
    open: for<'a> fn(&'a DeviceDescriptor) -> OpenFuture<'a>,
}

impl RegisteredAsyncDriver {
    fn new<D: AsyncTypedDeviceBackend>() -> Self {
        Self {
            driver: <D as AsyncTypedDeviceBackend>::driver(),
            #[cfg(target_arch = "wasm32")]
            webusb_filters: webusb_filters_typed::<D>,
            probe: probe_typed::<D>,
            open: open_typed::<D>,
        }
    }

    fn driver(&self) -> Driver {
        self.driver
    }

    fn probe<'a>(&self, args: &'a Args) -> ProbeFuture<'a> {
        (self.probe)(args)
    }

    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(&self, args: &Args) -> Result<Vec<WebUsbDeviceFilter>, Error> {
        (self.webusb_filters)(args)
    }

    fn open<'a>(&self, descriptor: &'a DeviceDescriptor) -> OpenFuture<'a> {
        (self.open)(descriptor)
    }
}

#[cfg(target_arch = "wasm32")]
fn webusb_filters_typed<D: AsyncTypedDeviceBackend>(
    args: &Args,
) -> Result<Vec<WebUsbDeviceFilter>, Error> {
    D::webusb_filters(args)
}

fn probe_typed<D: AsyncTypedDeviceBackend>(args: &Args) -> ProbeFuture<'_> {
    async move {
        D::async_probe(args).await.map(|descriptors| {
            descriptors
                .into_iter()
                .map(|args| DeviceDescriptor::new(<D as AsyncTypedDeviceBackend>::driver(), args))
                .collect()
        })
    }
    .boxed_async()
}

fn open_typed<D: AsyncTypedDeviceBackend>(descriptor: &DeviceDescriptor) -> OpenFuture<'_> {
    async move {
        Ok(DynAsyncDevice::from_impl(
            D::async_open(descriptor.args()).await?,
        ))
    }
    .boxed_async()
}

/// Registry of asynchronous driver discovery/opening backends.
pub struct AsyncRegistry {
    backends: Vec<RegisteredAsyncDriver>,
}

impl AsyncRegistry {
    /// Create an empty asynchronous registry.
    pub fn empty() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Register a typed built-in asynchronous driver.
    pub fn register<D>(&mut self) -> &mut Self
    where
        D: AsyncTypedDeviceBackend,
    {
        self.backends.push(RegisteredAsyncDriver::new::<D>());
        self
    }

    /// Request any user authorization needed by the selected async driver.
    ///
    /// Call this from a browser-window user gesture before probing, opening, or
    /// constructing a device-backed block in a Web Worker. If no driver is
    /// specified, one chooser includes filters from all registered WebUSB backends.
    #[cfg(target_arch = "wasm32")]
    pub async fn request_permission<'a, A>(&'a self, args: A) -> Result<(), Error>
    where
        A: TryInto<Args> + MaybeSend + 'a,
    {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let driver = requested_driver(&args)?;
        let mut matched_backend = false;
        let mut webusb_backends = Vec::new();
        let mut filters = Vec::new();

        for backend in &self.backends {
            if driver.is_some_and(|driver| driver != backend.driver()) {
                continue;
            }
            matched_backend = true;
            let backend_filters = backend.webusb_filters(&args)?;
            if backend_filters.is_empty() {
                continue;
            }
            webusb_backends.push(backend);
            for filter in backend_filters {
                if !filters.contains(&filter) {
                    filters.push(filter);
                }
            }
        }

        if let Some(driver) = driver {
            if !matched_backend {
                return Err(unavailable_driver(driver));
            }
        }
        if filters.is_empty() {
            return Err(Error::unsupported_reason(
                Capability::DriverOperation,
                "no matching async backend supports WebUSB permission requests",
            ));
        }

        for backend in webusb_backends {
            if !backend.probe(&args).await?.is_empty() {
                return Ok(());
            }
        }

        request_webusb_permission(&filters).await
    }

    /// Probe devices matching `args`.
    pub async fn probe<'a, A>(&'a self, args: A) -> Result<Vec<DeviceDescriptor>, Error>
    where
        A: TryInto<Args> + MaybeSend + 'a,
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
                descriptors.append(&mut backend.probe(&args).await?);
            }
        }

        if let Some(driver) = driver {
            if !matched_backend {
                return Err(unavailable_driver(driver));
            }
        }

        Ok(descriptors)
    }

    /// Open a discovered device descriptor.
    pub async fn open<'a>(
        &'a self,
        descriptor: &'a DeviceDescriptor,
    ) -> Result<DynAsyncDevice, Error> {
        let driver = descriptor.driver();
        let mut matched_backend = false;

        for backend in &self.backends {
            if backend.driver() != driver {
                continue;
            }
            matched_backend = true;
            match backend.open(descriptor).await {
                Ok(device) => return Ok(device),
                Err(Error::DeviceNotFound) => {}
                Err(e) => return Err(e),
            }
        }

        if !matched_backend {
            return Err(unavailable_driver(driver));
        }

        Err(Error::DeviceNotFound)
    }

    /// Open the first asynchronous device matching `args`.
    pub async fn open_args<'a, A>(&'a self, args: A) -> Result<DynAsyncDevice, Error>
    where
        A: TryInto<Args> + MaybeSend + 'a,
    {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let driver = requested_driver(&args)?;

        if let Some(driver) = driver {
            let descriptor = DeviceDescriptor::new(driver, args);
            return self.open(&descriptor).await;
        }

        for backend in &self.backends {
            let descriptor = DeviceDescriptor::new(backend.driver(), args.clone());
            match backend.open(&descriptor).await {
                Ok(device) => return Ok(device),
                Err(Error::DeviceNotFound) => {}
                Err(e) => return Err(e),
            }
        }

        Err(Error::DeviceNotFound)
    }
}

#[cfg(target_arch = "wasm32")]
async fn request_webusb_permission(filters: &[WebUsbDeviceFilter]) -> Result<(), Error> {
    let window = web_sys::window().ok_or_else(|| {
        Error::unsupported_reason(
            Capability::DriverOperation,
            "WebUSB permission requests require a browser Window",
        )
    })?;
    let usb = window.navigator().usb();
    if usb.is_undefined() {
        return Err(Error::unsupported_reason(
            Capability::DriverOperation,
            "WebUSB is not available",
        ));
    }

    let filters = filters
        .iter()
        .map(WebUsbDeviceFilter::to_web_sys)
        .collect::<Vec<_>>();
    let options = web_sys::UsbDeviceRequestOptions::new(&filters);
    wasm_bindgen_futures::JsFuture::from(usb.request_device(&options))
        .await
        .map_err(|_| Error::DeviceNotFound)?;
    Ok(())
}

impl Default for AsyncRegistry {
    fn default() -> Self {
        #[allow(unused_mut)]
        let mut registry = Self::empty();

        #[cfg(all(
            feature = "hackrf",
            any(target_arch = "wasm32", feature = "smol", feature = "tokio")
        ))]
        registry.register::<crate::impls::AsyncHackRf>();

        #[cfg(all(
            feature = "hydrasdr",
            any(target_arch = "wasm32", feature = "smol", feature = "tokio")
        ))]
        registry.register::<crate::impls::AsyncHydraSdr>();

        #[cfg(all(
            feature = "bladerf1",
            any(target_arch = "wasm32", feature = "smol", feature = "tokio")
        ))]
        registry.register::<crate::impls::AsyncBladeRf>();

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

fn unavailable_driver(driver: Driver) -> Error {
    if !matches!(
        driver,
        Driver::Dummy | Driver::HackRf | Driver::HydraSdr | Driver::BladeRf
    ) && crate::Registry::default().contains(driver)
    {
        Error::unsupported_reason(
            Capability::DriverOperation,
            format!("driver {driver:?} does not expose an async API"),
        )
    } else {
        Error::DriverFeatureNotEnabled { driver }
    }
}

#[cfg(all(
    test,
    feature = "dummy",
    any(feature = "hackrf", feature = "hydrasdr", feature = "bladerf1"),
    any(target_arch = "wasm32", feature = "smol", feature = "tokio")
))]
mod ordering_tests {
    use super::*;

    #[test]
    fn hardware_drivers_precede_the_dummy_fallback() {
        let drivers = AsyncRegistry::default()
            .backends
            .iter()
            .map(RegisteredAsyncDriver::driver)
            .collect::<Vec<_>>();
        let dummy = drivers
            .iter()
            .position(|driver| *driver == Driver::Dummy)
            .expect("dummy backend is enabled");
        for driver in [Driver::HackRf, Driver::HydraSdr, Driver::BladeRf] {
            if let Some(index) = drivers.iter().position(|candidate| *candidate == driver) {
                assert!(index < dummy, "{driver:?} should precede Dummy");
            }
        }
    }
}

#[cfg(all(
    test,
    any(feature = "hackrf", feature = "hydrasdr"),
    not(any(feature = "smol", feature = "tokio")),
    not(target_arch = "wasm32")
))]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    #[cfg(feature = "hackrf")]
    fn async_registry_reports_disabled_hackrf_without_runtime_feature() {
        block_on(async {
            let registry = AsyncRegistry::default();

            assert!(matches!(
                registry.probe("driver=hackrf").await,
                Err(Error::DriverFeatureNotEnabled {
                    driver: Driver::HackRf
                })
            ));
            assert!(matches!(
                registry.open_args("driver=hackrf").await,
                Err(Error::DriverFeatureNotEnabled {
                    driver: Driver::HackRf
                })
            ));
        });
    }

    #[test]
    #[cfg(feature = "hydrasdr")]
    fn async_registry_reports_disabled_hydrasdr_without_runtime_feature() {
        block_on(async {
            let registry = AsyncRegistry::default();

            assert!(matches!(
                registry.probe("driver=hydrasdr").await,
                Err(Error::DriverFeatureNotEnabled {
                    driver: Driver::HydraSdr
                })
            ));
            assert!(matches!(
                registry.open_args("driver=hydrasdr").await,
                Err(Error::DriverFeatureNotEnabled {
                    driver: Driver::HydraSdr
                })
            ));
        });
    }
}
