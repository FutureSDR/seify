use clap::Parser;
use num_complex::Complex32;
use vmcircbuffer::sync;

use seify::Device;
use seify::Direction::Rx;
use seify::Error;
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

    let dev = Device::from_args(cli.args)?;

    println!("driver:      {:?}", dev.driver());
    println!("id:          {:?}", dev.id()?);
    println!("info:        {:?}", dev.info()?);
    println!("sample rate: {:?}", dev.sample_rate(Rx, 0)?);
    println!("frequency:   {:?}", dev.frequency(Rx, 0)?);
    println!("gain:        {:?}", dev.gain(Rx, 0)?);

    let mut w = sync::Circular::with_capacity::<Complex32>(8192)?;
    let mut r = w.add_reader();

    // producer thread
    std::thread::spawn(move || -> Result<(), Error> {
        let mut rx = dev.rx_streamer(&[0])?;
        let mtu = rx.mtu()?;
        rx.activate(None)?;

        loop {
            let w_buff = w.slice();
            let n = std::cmp::min(w_buff.len(), mtu);
            let n = rx.read(&mut [&mut w_buff[0..n]], 200000)?;
            w.produce(n);
        }
    });

    // consumer
    loop {
        let r_buff = r.slice().unwrap();
        let l = r_buff.len();
        println!("received {l} samples");
        r.consume(l);
    }
}
