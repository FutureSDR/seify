#[cfg(feature = "aaronia")]
pub mod aaronia;
#[cfg(feature = "aaronia")]
pub use aaronia::Http;

#[cfg(feature = "hackrf")]
pub mod hackrf;
#[cfg(feature = "hackrf")]
pub use hackrf::HackRf;

#[cfg(feature = "rtlsdr")]
pub mod rtlsdr;
#[cfg(feature = "rtlsdr")]
pub use rtlsdr::RtlSdr;

#[cfg(feature = "soapy")]
pub mod soapy;
#[cfg(feature = "soapy")]
pub use soapy::Soapy;



pub enum DeviceType {
    #[cfg(feature = "aaronia")]
    AaroniaHttp,
    #[cfg(feature = "hackrf")]
    HackRf,
    #[cfg(feature = "rtlsdr")]
    RtlSdr,
    #[cfg(feature = "soapy")]
    Soapy,
}

pub trait DeviceImpl {}
pub struct TypedDevice<T> {
    _p: std::marker::PhantomData<T>,
}
pub struct Device {}

pub struct RXStreamer {}

pub struct TXStreamer {}


