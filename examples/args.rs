use serde::Deserialize;
use serde_with::serde_as;
use serde_with::DisplayFromStr;
use std::error::Error;

use seify::Args;

#[serde_as]
#[derive(Debug, Deserialize)]
struct Config {
    driver: String,
    #[serde_as(as = "DisplayFromStr")]
    id: u32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Args = "driver=\"the driver\", id=123, not=interesting".parse()?;
    println!("args:   {args:?}");

    let c: Config = args.deserialize().unwrap();
    println!("driver: {:?}", c.driver);
    println!("id:     {:?}", c.id);

    Ok(())
}
