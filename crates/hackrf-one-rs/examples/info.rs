use anyhow::{Context, Result};
use seify_hackrfone::HackRf;

fn main() -> Result<()> {
    let radio = HackRf::open_first().context("Failed to open Hackrf")?;

    println!("Board ID: {:?}", radio.board_id());
    println!("Version: {:?}", radio.version());
    println!("Device version: {:?}", radio.device_version());
    Ok(())
}
