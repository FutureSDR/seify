use clap::Parser;
use num_complex::Complex32;
use seify::DynDevice;
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
    /// RX center frequency in Hz
    #[clap(long)]
    frequency: Option<f64>,
    /// RX sample rate in samples per second
    #[clap(long)]
    sample_rate: Option<f64>,
    /// RX gain in dB
    #[clap(long)]
    gain: Option<f64>,
}

pub fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let cli = Args::parse();

    let dev = DynDevice::from_args(cli.args)?;

    println!("driver:      {:?}", dev.driver());
    println!("id:          {:?}", dev.id()?);
    println!("info:        {:?}", dev.info()?);
    {
        let rx0 = dev.rx(0)?;
        if let Some(frequency) = cli.frequency {
            rx0.frequency()?.set(frequency)?;
        }
        if let Some(sample_rate) = cli.sample_rate {
            rx0.sample_rate()?.set(sample_rate)?;
        }
        if let Some(gain) = cli.gain {
            if let Ok(agc) = rx0.agc() {
                agc.disable()?;
            }
            rx0.gain()?.set(gain)?;
        }

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
