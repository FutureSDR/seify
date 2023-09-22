use clap::Parser;
use num_complex::Complex32;
use std::error::Error;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use vmcircbuffer::sync;

use seify::Device;
use seify::Direction::Rx;
use seify::RxStreamer;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Device Filter
    #[clap(short, long, default_value = "")]
    args: String,
}

pub fn main() -> Result<(), Box<dyn Error>> {
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
    let terminate = Arc::new(AtomicBool::new(false));
    let rx_thread = std::thread::spawn({
        let terminate = terminate.clone();
        move || -> Result<(), Box<dyn Error + Send + Sync>> {
            let mut rx = dev.rx_streamer(&[0])?;
            let mtu = rx.mtu()?;
            rx.activate()?;

            loop {
                if terminate.load(Ordering::Relaxed) {
                    break Ok(());
                }
                let w_buff = w.slice();
                let n = std::cmp::min(w_buff.len(), mtu);
                let n = rx.read(&mut [&mut w_buff[0..n]], 200000)?;
                w.produce(n);
            }
        }
    });

    ctrlc::set_handler({
        let terminate = terminate.clone();
        move || {
            println!("terminating...");
            terminate.store(true, Ordering::Relaxed);
        }
    })
    .expect("Error setting Ctrl-C handler");

    // consumer
    loop {
        if terminate.load(Ordering::Relaxed) {
            break;
        }
        let r_buff = r.slice().unwrap();
        let l = r_buff.len();
        println!("received {l} samples");
        r.consume(l);
    }

    if let Err(e) = rx_thread.join() {
        std::panic::resume_unwind(e);
    }
    Ok(())
}
