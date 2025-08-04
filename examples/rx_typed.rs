use clap::Parser;
use num_complex::Complex32;

use seify::impls::rtlsdr;
use seify::Device;
use seify::Direction::Rx;
use seify::RxStreamer;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Device Filters
    #[clap(short, long, default_value = "")]
    args: String,
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let cli = Args::parse();

    let rtl = rtlsdr::RtlSdr::open(cli.args)?;
    let dev = Device::from_impl(rtl);
    // Get typed reference to device impl
    // let _r : &seify::impls::RtlSdr = dev.impl_ref().unwrap();

    dev.enable_agc(Rx, 0, true)?;
    dev.set_frequency(Rx, 0, 101e6)?;
    dev.set_sample_rate(Rx, 0, 3.2e6)?;

    println!("driver:      {:?}", dev.driver());
    println!("id:          {:?}", dev.id()?);
    println!("info:        {:?}", dev.info()?);
    println!("sample rate: {:?}", dev.sample_rate(Rx, 0)?);
    println!("frequency:   {:?}", dev.frequency(Rx, 0)?);
    println!("gain:        {:?}", dev.gain(Rx, 0)?);

    let mut samps = [Complex32::new(0.0, 0.0); 8192];
    let mut rx = dev.rx_streamer(&[0])?;
    rx.activate()?;
    let n = rx.read(&mut [&mut samps], 2000)?;

    plot(&mut samps[..n]);

    Ok(())
}

fn plot(s: &mut [num_complex::Complex32]) {
    use gnuplot::*;

    let mut planner = rustfft::FftPlanner::new();
    planner.plan_fft_forward(s.len()).process(s);

    let abs = s.iter().map(|s| s.norm_sqr().log10());

    let mut fg = Figure::new();
    fg.axes2d().set_title("Spectrum", &[]).lines(
        0..s.len(),
        abs,
        &[LineWidth(3.0), Color("blue".into()), LineStyle(DotDash)],
    );
    fg.show().unwrap();
}
