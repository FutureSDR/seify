use seify::enumerate;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devs = enumerate()?;
    println!("devs: {devs:?}");
    Ok(())
}
