use std::path::PathBuf;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "it8951-video",
    about = "Play videos on IT8951-controlled e-paper displays"
)]
struct Opt {
    /// Input video file.
    #[structopt(parse(from_os_str))]
    video: PathBuf,
}

fn main() {
    let opt = Opt::from_args();
    println!("{:?}", opt);
}
