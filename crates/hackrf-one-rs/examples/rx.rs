use anyhow::{Context, Result};
use num_complex::Complex32;
use seify_hackrfone::{Config, HackRf};
use std::time::Instant;

fn main() -> Result<()> {
    let context = rusb::Context::new().context("Failed to create rusb Context")?;
    let radio = HackRf::new(context).context("Failed to open Hackrf")?;

    println!("Board ID: {:?}", radio.board_id());
    println!("Version: {:?}", radio.version());
    println!("Device version: {:?}", radio.device_version());

    radio
        .start_rx(&Config {
            frequency_hz: 2_410_000_000,
            amp_enable: true,
            antenna_enable: false,
            ..Default::default()
        })
        .context("Failed to receive on hackrf")?;

    const MTU: usize = 128 * 1024;
    let mut buf = vec![0u8; MTU];
    let mut samples = vec![];

    let collect_count = 100_000_000;
    let mut last_print = Instant::now();

    while samples.len() < collect_count {
        let n = radio.rx(&mut buf).context("Failed to receive samples")?;
        assert_eq!(n, buf.len());
        for iq in buf.chunks_exact(2) {
            samples.push(Complex32::new(
                (iq[0] as f32 - 127.0) / 128.0,
                (iq[1] as f32 - 127.0) / 128.0,
            ));
        }

        if last_print.elapsed().as_millis() > 500 {
            println!(
                "  read {} samples ({:.1}%)",
                samples.len(),
                samples.len() as f64 / collect_count as f64 * 100.0
            );
            last_print = Instant::now();
        }
    }
    println!("Collected {} samples", samples.len());

    println!("First 100 {:#?} samples", &samples[..100]);

    Ok(())
}
