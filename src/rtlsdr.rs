use crate::DeviceTrait;

pub struct RtlSdr {}
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
