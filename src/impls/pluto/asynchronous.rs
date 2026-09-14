use super::{
    common::{map_error, probe_args, Metadata, Selector},
    IioContext,
};
use crate::{
    async_compat::Shared, dev::AsyncTypedDeviceBackend, Args, AsyncDeviceInfo, Direction, Driver,
    Error,
};
use futures::lock::Mutex;
use plutosdr::Device as PlutoDevice;

/// Asynchronous PlutoSDR context backend, including browser WebUSB.
/// Clones share one USB session and its shutdown state.
#[derive(Clone)]
pub struct AsyncPluto {
    session: Shared<Mutex<Option<PlutoDevice>>>,
    metadata: Shared<Metadata>,
}

impl AsyncPluto {
    /// Probe standard Pluto USB devices already authorized on browser targets.
    pub async fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let selector = Selector::from_args(args)?;
        let devices = PlutoDevice::list().await.map_err(map_error)?;
        Ok(selector
            .select(devices)
            .map(|(i, d)| probe_args(i, &d))
            .collect())
    }

    /// Open an IIO session and retrieve its XML context without RF configuration.
    /// On WebUSB, obtain permission through AsyncRegistry before opening.
    pub async fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = Selector::from_args(&args)?;
        let devices = PlutoDevice::list().await.map_err(map_error)?;
        let (index, descriptor) = selector
            .select(devices)
            .next()
            .ok_or(Error::DeviceNotFound)?;
        let device = PlutoDevice::builder()
            .descriptor(descriptor)
            .open()
            .await
            .map_err(map_error)?;
        let metadata = Shared::new(Metadata::from_device(&device, index)?);
        Ok(Self {
            session: Shared::new(Mutex::new(Some(device))),
            metadata,
        })
    }

    /// Cached context; inspecting it performs no USB I/O.
    pub fn context(&self) -> &IioContext {
        &self.metadata.context
    }

    /// Close the session shared by all clones; repeat/retry safely.
    /// The metadata snapshot remains readable after shutdown.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let mut session = self.session.lock().await;
        if let Some(device) = session.as_mut() {
            device.shutdown().await.map_err(map_error)?;
        }
        *session = None;
        Ok(())
    }
}

impl AsyncDeviceInfo for AsyncPluto {
    fn driver(&self) -> Driver {
        Driver::Pluto
    }
    async fn async_id(&self) -> Result<String, Error> {
        self.metadata.id()
    }
    async fn async_info(&self) -> Result<Args, Error> {
        Ok(self.metadata.args.clone())
    }
    async fn async_num_channels(&self, _direction: Direction) -> Result<usize, Error> {
        Ok(0)
    }
    async fn async_full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

crate::impl_dyn_async_device_backend!(AsyncPluto => []);

impl AsyncTypedDeviceBackend for AsyncPluto {
    fn driver() -> Driver {
        Driver::Pluto
    }
    async fn async_probe(args: &Args) -> Result<Vec<Args>, Error> {
        Self::probe(args).await
    }
    async fn async_open(args: &Args) -> Result<Self, Error> {
        Self::open(args.clone()).await
    }

    #[cfg(target_arch = "wasm32")]
    fn webusb_filters(args: &Args) -> Result<Vec<crate::dev::WebUsbDeviceFilter>, Error> {
        use plutosdr::usb::discovery::{PLUTO_PID, PLUTO_VID};
        let mut filter =
            crate::dev::WebUsbDeviceFilter::new().with_vendor_product(PLUTO_VID, PLUTO_PID);
        if let Selector::Serial(serial) = Selector::from_args(args)? {
            filter = filter.with_serial_number(serial);
        }
        Ok(vec![filter])
    }
}
