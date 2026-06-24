use clap::Parser;
use seify::enumerate_with_args;
use seify::Device;

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
        let rx0 = dev.rx(0)?;
        println!("sample rate:  {:?}", rx0.sample_rate()?.value()?);
        println!("frequency:    {:?}", rx0.frequency()?.value()?);
        println!("gain:         {:?}", rx0.gain()?.value()?);
        println!("gain range:   {:?}", rx0.gain()?.range()?);
        println!("sample rate range: {:?}", rx0.sample_rate()?.range()?);
    }

    Ok(())
}
