use super::{
    common::{map_error, probe_args, Metadata, Selector},
    IioContext,
};
use crate::{Args, DeviceInfo, Direction, Driver, Error};
use plutosdr::{Device as PlutoDevice, MaybeFuture};
use std::sync::{Arc, Mutex};

/// Native PlutoSDR context backend. Clones share one USB session.
#[derive(Clone)]
pub struct Pluto {
    session: Arc<Mutex<Option<PlutoDevice>>>,
    metadata: Arc<Metadata>,
}

impl Pluto {
    /// Enumerate standard Pluto USB devices, optionally selecting serial or index.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        let selector = Selector::from_args(args)?;
        let devices = PlutoDevice::list().wait().map_err(map_error)?;
        Ok(selector
            .select(devices)
            .map(|(i, d)| probe_args(i, &d))
            .collect())
    }

    /// Open the named IIO interface and read its context, without RF configuration.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;
        let selector = Selector::from_args(&args)?;
        let devices = PlutoDevice::list().wait().map_err(map_error)?;
        let (index, descriptor) = selector
            .select(devices)
            .next()
            .ok_or(Error::DeviceNotFound)?;
        let device = PlutoDevice::builder()
            .descriptor(descriptor)
            .open()
            .wait()
            .map_err(map_error)?;
        let metadata = Arc::new(Metadata::from_device(&device, index)?);
        Ok(Self {
            session: Arc::new(Mutex::new(Some(device))),
            metadata,
        })
    }

    /// Cached context with IIO devices, channels, attributes, and scan layouts.
    pub fn context(&self) -> &IioContext {
        &self.metadata.context
    }

    /// Close the shared USB session for all clones; safe to repeat or retry.
    /// The metadata snapshot remains readable after shutdown.
    pub fn shutdown(&self) -> Result<(), Error> {
        let mut session = self.session.lock().map_err(|_| Error::DeviceDisconnected)?;
        if let Some(device) = session.as_mut() {
            device.shutdown().wait().map_err(map_error)?;
        }
        *session = None;
        Ok(())
    }
}

impl DeviceInfo for Pluto {
    fn driver(&self) -> Driver {
        Driver::Pluto
    }
    fn id(&self) -> Result<String, Error> {
        self.metadata.id()
    }
    fn info(&self) -> Result<Args, Error> {
        Ok(self.metadata.args.clone())
    }
    fn num_channels(&self, _direction: Direction) -> Result<usize, Error> {
        Ok(0)
    }
    fn full_duplex(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

crate::impl_dyn_device_backend!(Pluto => []);
crate::registry::impl_typed_device_backend!(Pluto, Driver::Pluto);
