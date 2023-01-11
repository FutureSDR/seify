use crate::DeviceTrait;

pub struct HackRf {}
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
