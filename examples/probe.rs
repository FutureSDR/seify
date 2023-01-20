use seify::impls::rtlsdr;
use seify::enumerate;
use seify::Device;
use seify::DeviceTrait;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // let devs = enumerate()?;
    // println!("devs: {devs:?}");
    println!("opening");
    let rtl = rtlsdr::RtlSdr::open("")?;
    println!("opened");
    let dev = Device::from_device(rtl);

    println!("getting driver");
    println!("driver: {:?}", dev.driver());
    println!("id: {:?}", dev.id()?);
    println!("info: {:?}", dev.info()?);

    Ok(())
}
