use std::any::Any;

use crate::Args;
use crate::Driver;
use crate::Error;

pub trait DeviceTrait {
    fn driver(&self) -> Driver;
    fn id(&self) -> Result<String, Error>;
    fn info(&self) -> Result<Args, Error>;
}

pub struct Device<T: DeviceTrait + Any> {
    dev: T,
}

impl Device<Box<dyn DeviceTrait>> {
    pub fn new() -> Result<Self, Error> {
        let mut devs = crate::enumerate()?;
        if devs.is_empty() {
            return Err(Error::NotFound);
        }
        Self::from_args(devs.remove(0))
    }

    pub fn from_args<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args.try_into().or(Err(Error::ValueError))?;
        let driver = match args.get::<Driver>("driver") {
            Ok(d) => Some(d),
            Err(Error::NotFound) => None,
            Err(e) => return Err(e),
        };
        if cfg!(feature = "rtlsdr") && (driver.is_none() || matches!(driver, Some(Driver::RtlSdr)))
        {
            return Ok(Device {
                dev: Box::new(crate::RtlSdr::open(&args)?),
            });
        }
        if cfg!(feature = "hackrf") && (driver.is_none() || matches!(driver, Some(Driver::HackRf)))
        {
            return Ok(Device {
                dev: Box::new(crate::HackRf::open(&args)?),
            });
        }
        Err(Error::NotFound)
    }
}

impl<T: DeviceTrait + Any> Device<T> {
    pub fn from_device(dev: T) -> Self {
        Self { dev }
    }
    pub fn inner<D: Any>(&mut self) -> Result<&mut D, Error> {
        (&mut self.dev as &mut dyn Any)
            .downcast_mut::<D>()
            .ok_or(Error::ValueError)
    }
}

impl<T: DeviceTrait + 'static> DeviceTrait for Device<T> {
    fn driver(&self) -> Driver {
        self.dev.driver()
    }

    fn id(&self) -> Result<String, Error> {
        self.dev.id()
    }

    fn info(&self) -> Result<Args, Error> {
        self.dev.info()
    }
}

impl DeviceTrait for Box<dyn DeviceTrait> {
    fn driver(&self) -> Driver {
        self.as_ref().driver()
    }
    fn id(&self) -> Result<String, Error> {
        self.as_ref().id()
    }
    fn info(&self) -> Result<Args, Error> {
        self.as_ref().info()
    }
}
