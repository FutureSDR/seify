use clap::Parser;
use num_complex::Complex32;

use seify::DynDevice;
use seify::RxStreamer;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Device Filters
    #[clap(short, long, default_value = "")]
    args: String,
    /// RX center frequency in Hz
    #[clap(long)]
    frequency: Option<f64>,
    /// RX sample rate in samples per second
    #[clap(long)]
    sample_rate: Option<f64>,
    /// RX gain in dB
    #[clap(long)]
    gain: Option<f64>,
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let cli = Args::parse();

    // HydraSDR RFOne is rx-only and can be selected with generic args, e.g.:
    // cargo run --no-default-features --features hydrasdr --example rx_generic -- --args driver=hydrasdr
    let dev = DynDevice::from_args(cli.args)?;
    // Get typed reference to device impl
    // let r: &seify::impls::RtlSdr = dev.downcast_ref().unwrap();

    let rx0 = dev.rx(0)?;
    if let Some(frequency) = cli.frequency {
        rx0.frequency()?.set(frequency)?;
    }
    if let Some(sample_rate) = cli.sample_rate {
        rx0.sample_rate()?.set(sample_rate)?;
    }
    if let Some(gain) = cli.gain {
        if let Ok(agc) = rx0.agc() {
            agc.disable()?;
        }
        rx0.gain()?.set(gain)?;
    }

    println!("driver:      {:?}", dev.driver());
    println!("id:          {:?}", dev.id()?);
    println!("info:        {:?}", dev.info()?);
    println!("sample rate: {:?}", rx0.sample_rate()?.value()?);
    println!("frequency:   {:?}", rx0.frequency()?.value()?);
    println!("gain:        {:?}", rx0.gain()?.value()?);

    let mut samps = [Complex32::new(0.0, 0.0); 8192];
    let mut rx = rx0.streamer()?;
    rx.activate()?;
    let n = rx.read(&mut [&mut samps], 200000)?;

    plot(&mut samps[..n]);

    Ok(())
}

fn plot(s: &mut [num_complex::Complex32]) {
    use gnuplot::*;

    let mut planner = rustfft::FftPlanner::new();
    planner.plan_fft_forward(s.len()).process(s);

    let abs: Vec<f32> = s.iter().map(|s| s.norm_sqr().log10()).collect();

    let mut fg = Figure::new();
    fg.axes2d().set_title("Spectrum", &[]).lines(
        0..s.len(),
        abs,
        &[LineWidth(3.0), Color("blue".into()), LineStyle(DotDash)],
    );
    fg.show().unwrap();
}
