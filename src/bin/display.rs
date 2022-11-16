use std::fs;
use std::path::PathBuf;

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

    /// Input video width.
    #[structopt(short = "w", long = "width", default_value = "1856")]
    width: u32,

    /// Input video height.
    #[structopt(short = "h", long = "height", default_value = "1392")]
    height: u32,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let width = opt.width;
    let height = opt.height;

    // Read raw frame data
    let data = fs::read(opt.input)?;
    let frames: RawFrames = bincode::deserialize(&data)?;

    // Connect to IT8951 controlled display
    let mut api = API::connect(width, height)?;

    // Get system information
    let system_info = api.get_system_info();
    let image_buffer_base = system_info.image_buffer_base;

    // Calculate byte size of each 1bpp image
    let image_size = (width * height) / 8;

    println!(
        r#"
Panel Dimensions: {}x{}
Video Dimensions: {}x{}
  Buffer Address: 0x{:x}
      Image size: {} bytes
        "#,
        system_info.width, system_info.height, width, height, image_buffer_base, image_size
    );

    // Make sure the file and display dimension actually match
    assert_eq!(frames.width(), width);
    assert_eq!(frames.height(), height);
    assert_eq!(frames.get(0).len(), image_size as usize);

    // Send SCSI inquiry command
    // @TODO: Can this be removed?
    api.inquiry()?;

    // Set VCOM value
    api.set_vcom(1_580)?; // -1.58

    // Write images to buffer
    api.load_image_area(image_buffer_base + (image_size * 0), &frames.get(0))?;
    api.load_image_area(image_buffer_base + (image_size * 1), &frames.get(1))?;
    api.load_image_area(image_buffer_base + (image_size * 2), &frames.get(2))?;
    api.load_image_area(image_buffer_base + (image_size * 3), &frames.get(3))?;

    // Enable 1bit drawing and image pitch mode
    // 0000 0000 0000 0110 0000 0000 0000 0000
    // |         |     ^^  |         |
    // 113B      113A      1139      1138
    let reg = api.get_memory_register_value(0x1800_1138)?;
    api.set_memory_register_value(0x1800_1138, reg | (1 << 18) | (1 << 17))?;

    // Set bitmap mode color definition (0 - set black(0x00), 1 - set white(0xf0))
    api.set_memory_register_value(0x1800_1250, 0xf0 | (0x00 << 8))?;

    // Set image pitch width
    api.set_memory_register_value(0x1800_124c, (width / 8) / 4)?;

    // Draw first image properly (this is slower)
    api.display_image(image_buffer_base + (image_size * 0), Mode::GL16)?;

    // ... and display the others with a faster mode
    api.display_image(image_buffer_base + (image_size * 0), Mode::A2)?;
    api.display_image(image_buffer_base + (image_size * 1), Mode::A2)?;
    api.display_image(image_buffer_base + (image_size * 2), Mode::A2)?;
    api.display_image(image_buffer_base + (image_size * 3), Mode::A2)?;

    // Reset register to original value
    api.set_memory_register_value(0x1800_1138, reg)?;

    // Clean up afterwards, by setting screen to white
    api.clear_display()?;

    Ok(())
}
