#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), seify::Error> {
    use num_complex::Complex32;
    use seify::{impls::Pluto, Device, RxStreamer};
    let device = Device::<Pluto>::from_args("driver=pluto")?;
    let channel = device.rx(0)?;
    channel.frequency().set(2_450_000_000.0)?;
    channel.sample_rate().set(2_500_000.0)?;
    channel.bandwidth().set(2_000_000.0)?;
    channel.gain().set(30.0)?;
    println!(
        "RX: {} Hz, {} samples/s, {:?} dB",
        channel.frequency().value()?,
        channel.sample_rate().value()?,
        channel.gain().value()?
    );
    let mut rx = device.rx_streamer(&[0])?;
    let mut samples = vec![Complex32::default(); rx.mtu()?];
    rx.activate()?;
    for _ in 0..16 {
        let count = rx.read(&mut [&mut samples], 1_000_000)?;
        let power = samples[..count]
            .iter()
            .map(|s| s.norm_sqr() as f64)
            .sum::<f64>()
            / count as f64;
        println!("{count} samples; mean power {power:.8}");
    }
    rx.deactivate()?;
    drop(rx);
    device.as_inner().shutdown()
}
#[cfg(target_arch = "wasm32")]
fn main() {}
