use crate::Args;
use crate::DeviceTrait;
use crate::Error;

pub struct RtlSdr {}

impl RtlSdr {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        Ok(Vec::new())
    }
}

impl DeviceTrait for RtlSdr {
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
