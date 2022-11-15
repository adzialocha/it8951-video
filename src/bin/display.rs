use std::path::PathBuf;
use std::thread;
use std::{fs, time::Duration};

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

    // Get system information
    let system_info = api.get_system_info();
    let base_address = system_info.image_buffer_base;
    let width = system_info.width;
    let height = system_info.height;
    let image_size = width * height / 8;

    // Send SCSI inquiry command
    api.inquiry()?;

    // Set VCOM value
    api.set_vcom(1_580)?; // -1.58

    // Clear screen first
    /* api.display_image(base_address, Mode::INIT)?;
    thread::sleep(Duration::from_millis(3500)); */

    // Enable 1bit drawing and image pitch mode
    let reg = api.get_memory_register_value(0x1800_1138)?;
    api.set_memory_register_value(0x1800_1138, reg | (1 << 18) | (1 << 17))?;

    // Set image pitch width
    api.set_memory_register_value(0x1800_124c, frames.width() / 8 / 4)?;

    // Set bitmap mode color definition (0 - set black(0x00), 1 - set white(0xf0))
    api.set_memory_register_value(0x1800_1250, 0xf0 | (0x00 << 8))?;

    // Make sure the file and display dimension actually match
    assert_eq!(frames.width(), width);
    assert_eq!(frames.height(), height);

    // Write images to buffer
    api.fast_write_to_memory(base_address + (image_size * 0), &frames.get(4))?;
    // api.fast_write_to_memory(base_address + (image_size * 1), &frames.get(1))?;
    // api.fast_write_to_memory(base_address + (image_size * 2), &frames.get(2))?;
    // api.fast_write_to_memory(base_address + (image_size * 3), &frames.get(3))?;

    // ... and display them
    api.display_image(base_address + (image_size * 0), Mode::A2)?;
    // api.display_image(base_address + (image_size * 1), Mode::A2)?;
    // api.display_image(base_address + (image_size * 2), Mode::A2)?;
    // api.display_image(base_address + (image_size * 3), Mode::A2)?;

    Ok(())
}
