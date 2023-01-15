use crate::Args;
use crate::DeviceTrait;
use crate::Error;

pub struct HackRf {}

impl HackRf {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        Ok(Vec::new())
    }
    pub fn open(_args: &Args) -> Result<Self, Error> {
        Err(Error::NotFound)
    }
}

impl DeviceTrait for HackRf {
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
