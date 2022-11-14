use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use structopt::StructOpt;

use it8951_video::RawFrames;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "it8951-video-display",
    about = "Play videos on IT8951-controlled e-paper displays"
)]
struct Opt {
    /// Input raw data file.
    #[structopt(parse(from_os_str))]
    input: PathBuf,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let data = fs::read(opt.input).unwrap();
    let frames: RawFrames = bincode::deserialize(&data)?;
    println!(
        "width = {}px, height = {}px",
        frames.width(),
        frames.height()
    );
    Ok(())
}
