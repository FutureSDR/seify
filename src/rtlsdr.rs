use crate::Args;
use crate::DeviceTrait;
use crate::Error;

pub struct RtlSdr {}

impl RtlSdr {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        Ok(Vec::new())
    }
    pub fn open(_args: &Args) -> Result<Self, Error> {
        todo!()
    }
}

impl DeviceTrait for RtlSdr {
    fn driver(&self) -> crate::Driver {
        todo!()
    }

    fn id(&self) -> Result<String, Error> {
        todo!()
    }

    fn info(&self) -> Result<Args, Error> {
        todo!()
    }
}
