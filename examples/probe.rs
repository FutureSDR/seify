use clap::Parser;
use seify::enumerate_with_args;
use seify::Device;
use seify::Direction::Rx;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Device Filters
    #[clap(short, long, default_value = "")]
    args: String,
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    // cargo run --features=bladerf1 --no-default-features --example probe
    env_logger::init();
    let cli = Args::parse();

    let devs = enumerate_with_args(cli.args)?;
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
