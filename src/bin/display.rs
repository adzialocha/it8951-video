use std::fs;
use std::path::PathBuf;
use std::thread;

use anyhow::Result;
use structopt::StructOpt;

use it8951_video::{Mode, RawFrames, API};

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

    // Read raw frame data
    let data = fs::read(opt.input)?;
    let frames: RawFrames = bincode::deserialize(&data)?;

    // Connect to IT8951 controlled display
    let mut api = API::connect()?;
    api.inquiry()?;

    let system_info = api.get_system_info();
    println!("{:#?}", system_info);

    // Make sure the file and display dimension actually match
    assert_eq!(frames.width(), system_info.width);
    assert_eq!(frames.height(), system_info.height);
    println!(
        "width = {}px, height = {}px",
        frames.width(),
        frames.height()
    );

    let base_address = system_info.image_buffer_base;
    let image_size = (system_info.width / 8 + 1) * system_info.height;
    println!("{:?}", image_size);

    // api.reset()?;
    // api.display_image(Mode::INIT, base_address)?;
    // thread::sleep(std::time::Duration::from_millis(1000));

    api.preload_image(frames.frame(0), base_address)?;
    api.display_image(Mode::A2, base_address)?;
    api.preload_image(frames.frame(1), base_address)?;
    api.display_image(Mode::A2, base_address)?;
    api.preload_image(frames.frame(2), base_address)?;
    api.display_image(Mode::A2, base_address)?;
    api.preload_image(frames.frame(4), base_address)?;
    api.display_image(Mode::A2, base_address)?;

    Ok(())
}
