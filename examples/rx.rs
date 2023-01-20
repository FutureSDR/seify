use num_complex::Complex32;

use seify::impls::rtlsdr;
use seify::Args;
use seify::Device;
use seify::DeviceTrait;
use seify::Direction::Rx;
use seify::RxStreamer;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let rtl = rtlsdr::RtlSdr::open("")?;
    let dev = Device::from_device(rtl);
    dev.enable_agc(Rx, 0, true)?;
    dev.set_frequency(Rx, 0, 101e6, Args::new())?;
    dev.set_sample_rate(Rx, 0, 3.2e6)?;

    println!("driver:      {:?}", dev.driver());
    println!("id:          {:?}", dev.id()?);
    println!("info:        {:?}", dev.info()?);
    println!("sample rate: {:?}", dev.sample_rate(Rx, 0)?);
    println!("frequency:   {:?}", dev.frequency(Rx, 0)?);
    println!("gain:        {:?}", dev.gain(Rx, 0)?);

    let mut samps = [Complex32::new(0.0, 0.0); 8192];
    let mut rx = dev.rx_stream(&[0])?;
    rx.activate(None)?;
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
        &[LineWidth(3.0), Color("blue"), LineStyle(DotDash)],
    );
    fg.show().unwrap();
}
