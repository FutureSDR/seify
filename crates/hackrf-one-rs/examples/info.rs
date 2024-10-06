use seify_hackrfone::HackRf;

fn main() {
    let radio = HackRf::open_first().expect("Failed to open Hackrf");

    println!("Board ID: {:?}", radio.board_id());
    println!("Version: {:?}", radio.version());
    println!("Device version: {:?}", radio.device_version());
}
