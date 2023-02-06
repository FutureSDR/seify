#![allow(dead_code)]
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
    // create Args from string
    let mut args = Args::from("driver=\"the driver\", id=123, not=interesting")?;
    // set value manually
    args.set("bar", "baz");
    // merge with other args
    args.merge(Args::from("foo = bar")?);
    println!("args: {args:?}");

    // get a value, parsing it from the value string
    println!("id {}", args.get::<u32>("id").unwrap());

    // deserialize a struct from the arguments
    let c: Config = args.deserialize().unwrap();
    println!("config {c:#?}");

    Ok(())
}
