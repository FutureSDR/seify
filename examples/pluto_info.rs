#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), seify::Error> {
    use seify::{impls::Pluto, Device, Registry};
    let descriptors = Registry::default().probe("driver=pluto")?;
    println!("Pluto USB devices: {descriptors:?}");
    let descriptor = descriptors.first().ok_or(seify::Error::DeviceNotFound)?;
    let device = Device::<Pluto>::from_args(descriptor.args().clone())?;
    println!("Info: {:?}", device.info()?);
    println!("Capabilities: {:?}", device.capabilities()?);
    for iio in &device.as_inner().context().devices {
        println!(
            "{} {:?}: {} IIO channels",
            iio.id,
            iio.name,
            iio.channels.len()
        );
    }
    device.as_inner().shutdown()
}
