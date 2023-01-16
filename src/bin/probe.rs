use seify::impls::rtlsdr;
use seify::enumerate;
use seify::Device;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devs = enumerate()?;
    println!("devs: {devs:?}");

    let rtl = rtlsdr::RtlSdr::open("")?;
    let _d_rtl = Device::from_device(rtl);
    let _g_dev = Device::new();

    Ok(())
}
