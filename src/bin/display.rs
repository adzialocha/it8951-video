use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, sync::Arc};

use anyhow::Result;
use itertools::Itertools;
use structopt::StructOpt;

use it8951_video::{Mode, RawFrames, API};

/// Number of frames being loaded into memory before displaying.
const BUFFER_SIZE: usize = 8;

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

    /// Render every nth frame from video data.
    #[structopt(short = "t", long = "take", default_value = "4")]
    take: usize,

    /// VCOM value.
    #[structopt(short = "v", long = "vcom", default_value = "-1.58")]
    vcom: f32,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    // Prepare handler which informs us about exit when [CTRL] + [C] got pressed
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        running_clone.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Read raw frame data
    let data = fs::read(opt.input)?;
    let frames: RawFrames = bincode::deserialize(&data)?;

    // Connect to IT8951 controlled display
    let mut api = API::connect(opt.width, opt.height)?;

    // Get system information
    let system_info = api.get_system_info();
    let image_buffer_base = system_info.image_buffer_base;

    // Calculate byte size of each 1bpp image
    let image_size = (opt.width * opt.height) / 8;

    println!(
        r#"► Play video on e-paper

      VCOM value: {}
Panel Dimensions: {}x{}
Video Dimensions: {}x{}
  Buffer Address: 0x{:x}
      Image size: {} bytes
        "#,
        opt.vcom,
        system_info.width,
        system_info.height,
        opt.width,
        opt.height,
        image_buffer_base,
        image_size
    );

    // Make sure the file and display dimension actually match
    assert_eq!(frames.width(), opt.width);
    assert_eq!(frames.height(), opt.height);
    assert_eq!(
        frames.get(0).expect("No data given in video file").len(),
        image_size as usize
    );

    // Set VCOM value
    assert!(opt.vcom > -5.0 && opt.vcom < 0.0);
    api.set_vcom(opt.vcom)?;

    // Remember register value for later
    let reg = api.get_memory_register_value(0x1800_1138)?;

    // Write images to buffer
    assert!(opt.take > 0);
    for frames_chunk in &frames
        .iter()
        .enumerate()
        .skip_while(|(i, _)| i % opt.take != 0)
        .chunks(BUFFER_SIZE)
    {
        let mut frames_count = 0;

        // Exit here if user decided to quit program early
        if !running.load(Ordering::SeqCst) {
            break;
        }

        // Load images into buffer
        for (index, frame) in frames_chunk.enumerate() {
            api.set_memory(image_buffer_base + (image_size * index as u32), &frame.1)?;
            frames_count += 1;
        }

        // Enable 1bit drawing and image pitch mode
        // 0000 0000 0000 0110 0000 0000 0000 0000
        // |         |     ^^  |         |
        // 113B      113A      1139      1138
        api.set_memory_register_value(0x1800_1138, reg | (1 << 18) | (1 << 17))?;

        // Set bitmap mode color definition (0 - set black(0x00), 1 - set white(0xf0))
        api.set_memory_register_value(0x1800_1250, 0xf0 | (0x00 << 8))?;

        // Set image pitch width
        api.set_memory_register_value(0x1800_124c, (opt.width / 8) / 4)?;

        // Draw first image properly (this is slower) to avoid too much ghosting
        api.display_image(image_buffer_base + (image_size * 0), Mode::GL16)?;

        // ... and display the others with a faster mode
        if frames_count > 1 {
            for index in 1..frames_count {
                api.display_image(image_buffer_base + (image_size * index as u32), Mode::A2)?;
            }
        }
    }

    // Reset register to original value
    api.set_memory_register_value(0x1800_1138, reg)?;

    // Clean up afterwards, by setting screen to white
    api.clear_display()?;

    Ok(())
}
