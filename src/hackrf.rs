use crate::Args;
use crate::DeviceTrait;
use crate::Error;

pub struct HackRf {}

impl HackRf {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        Ok(Vec::new())
    }
}

impl DeviceTrait for HackRf {
    fn driver(&self) -> crate::Driver {
        todo!()
    }

    fn serial(&self) -> Option<String> {
        todo!()
    }

    fn url(&self) -> Option<String> {
        todo!()
    }
}
