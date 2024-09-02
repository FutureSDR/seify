use seify_hackrfone::HackRf;

fn main() {
    let context = rusb::Context::new().expect("Failed to create rusb Context");
    let radio = HackRf::new(context).expect("Failed to open Hackrf");

    println!("Board ID: {:?}", radio.board_id());
    println!("Version: {:?}", radio.version());
    println!("Device version: {:?}", radio.device_version());
}
