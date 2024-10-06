use anyhow::{Context, Result};
use num_complex::Complex32;
use seify_hackrfone::{Config, HackRf};
use std::time::Instant;

fn main() -> Result<()> {
    let radio = HackRf::open_first().context("Failed to open Hackrf")?;

    println!("Board ID: {:?}", radio.board_id());
    println!("Version: {:?}", radio.version());
    println!("Device version: {:?}", radio.device_version());

    radio
        .start_rx(&Config {
            vga_db: 0,
            txvga_db: 0,
            lna_db: 0,
            power_port_enable: false,
            antenna_enable: false,
            frequency_hz: 915_000_000,
            sample_rate_hz: 2_000_000,
            sample_rate_div: 0,
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
