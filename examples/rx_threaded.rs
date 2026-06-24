use clap::Parser;
use num_complex::Complex32;
use seify::Device;
use seify::RxStreamer;
use std::error::Error;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use vmcircbuffer::sync::Circular;

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

    let dev: Device = Device::from_args(cli.args)?;

    println!("driver:      {:?}", dev.driver());
    println!("id:          {:?}", dev.id()?);
    println!("info:        {:?}", dev.info()?);
    {
        let rx0 = dev.rx(0)?;
        println!("sample rate: {:?}", rx0.sample_rate()?.value()?);
        println!("frequency:   {:?}", rx0.frequency()?.value()?);
        println!("gain:        {:?}", rx0.gain()?.value()?);
    }

    let mut w = Circular::with_capacity::<Complex32>(8192)?;
    let mut r = w.add_reader();

    // producer thread
    let terminate = Arc::new(AtomicBool::new(false));
    let rx_thread = std::thread::spawn({
        let terminate = terminate.clone();
        move || -> Result<(), Box<dyn Error + Send + Sync>> {
            let rx0 = dev.rx(0)?;
            let mut rx = rx0.streamer()?;
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
        let Some(r_buff) = r.slice() else {
            break;
        };
        let l = r_buff.len();
        println!("received {l} samples");
        r.consume(l);
    }

    match rx_thread.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(e) => std::panic::resume_unwind(e),
    }
    Ok(())
}
