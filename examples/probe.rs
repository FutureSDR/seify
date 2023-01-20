use seify::enumerate;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Rx;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let devs = enumerate()?;
    println!("Devices");
    println!("=========================================");
    println!("devs: {devs:?}");

    for d in devs {
        let dev = Device::from_args(d)?;

        println!();
        println!("Device ({:?} - {:?}), ", dev.driver(), dev.id()?);
        println!("=========================================");

        println!("driver:       {:?}", dev.driver());
        println!("id:           {:?}", dev.id()?);
        println!("info:         {:?}", dev.info()?);
        println!("sample rate:  {:?}", dev.sample_rate(Rx, 0)?);
        println!("frequency:    {:?}", dev.frequency(Rx, 0)?);
        println!("gain:         {:?}", dev.gain(Rx, 0)?);
        println!("gain range:   {:?}", dev.gain_range(Rx, 0)?);
        println!("sample rate range: {:?}", dev.get_sample_rate_range(Rx, 0)?);
    }

    Ok(())
}
