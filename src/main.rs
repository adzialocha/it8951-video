mod api;
mod usb;

use std::path::PathBuf;

use anyhow::Result;
use ffmpeg_next::format::{input, Pixel};
use ffmpeg_next::media::Type;
use ffmpeg_next::software::scaling::{context::Context, flag::Flags};
use ffmpeg_next::util::frame::video::Video;
use image::GenericImageView;
use itertools::Itertools;
use structopt::StructOpt;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::task;

use api::{Mode, API};

/// Single video frame to be displayed on e-paper. It contains multiple bytes where every bit of it
/// represents a pixel (1 = white, 0 = black).
type Frame = Vec<u8>;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "it8951-video-display",
    about = "Play videos on IT8951-controlled e-paper displays"
)]
struct Opt {
    /// Video file which will be displayed.
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Width of video on display.
    #[structopt(short = "w", long = "width", default_value = "1856")]
    width: u32,

    /// Height of video on display.
    #[structopt(short = "h", long = "height", default_value = "1392")]
    height: u32,

    /// Only take every nth frame from video.
    #[structopt(short = "t", long = "take", default_value = "5")]
    take: usize,

    /// Paint in GL16 mode every nth frame.
    #[structopt(short = "g", long = "ghost", default_value = "32")]
    ghost: usize,

    /// VCOM value.
    #[structopt(short = "v", long = "vcom", default_value = "-1.58")]
    vcom: f32,
}

struct ThresholdMatrix {
    nx: u32,
    ny: u32,
    matrix: Vec<u8>,
}

impl ThresholdMatrix {
    fn new() -> Self {
        let texture = image::load(
            std::io::Cursor::new(include_bytes!("blue-noise.png")),
            image::ImageFormat::Png,
        )
        .unwrap()
        .grayscale();
        let dim = texture.dimensions();

        let matrix = (0..dim.0)
            .cartesian_product(0..dim.1)
            .map(|(x, y)| texture.get_pixel(x, y)[0])
            .collect();

        Self {
            nx: dim.0,
            ny: dim.1,
            matrix,
        }
    }

    fn look_up(&self, x: u32, y: u32) -> u8 {
        let j = x % self.nx;
        let i = y % self.ny;
        let idx: usize = (i * self.nx + j)
            .try_into()
            .expect("i * side_length + j does not fit into usize");

        self.matrix[idx]
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Opt::from_args();

    // Establish communication channels between both threads
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<bool>(1);
    let (frame_tx, mut frame_rx) = mpsc::unbounded_channel::<Frame>();

    // Spawn the first thread: It will decode the video, convert every frame into the right format
    // and send it over to the display thread.
    task::spawn_blocking(move || {
        // Open video stream from file
        let mut context_video = input(&opt.input).unwrap();

        let input = context_video
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg_next::Error::StreamNotFound)
            .unwrap();
        let video_stream_index = input.index();

        // Prepare video decoder which should rescale frames to target size and make them grayscale
        let context_decoder =
            ffmpeg_next::codec::context::Context::from_parameters(input.parameters()).unwrap();
        let mut decoder = context_decoder.decoder().video().unwrap();

        let mut scaler = Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::GRAY8,
            opt.width,
            opt.height,
            Flags::BILINEAR,
        )
        .unwrap();

        let threshold_matrix = ThresholdMatrix::new();
        let mut frame_counter = 0;
        assert!(opt.take > 0);

        let mut receive_and_process_decoded_frames =
            |decoder: &mut ffmpeg_next::decoder::Video| -> Result<(), ffmpeg_next::Error> {
                let mut decoded = Video::empty();

                while decoder.receive_frame(&mut decoded).is_ok() {
                    // Decode next frame
                    let mut frame = Video::empty();
                    scaler.run(&decoded, &mut frame)?;

                    // Only take every nth frame from video
                    if frame_counter % opt.take == 0 {
                        // Encode as grayscale image
                        let data_8bpp = frame.data(0);
                        let width = frame.width();
                        let height = frame.height();

                        // Dither and convert to raw format, representing black (0) or white (1) pixels
                        // in an array
                        let base_two: u8 = 2;
                        let mut data_1bpp: Frame = vec![0b0000_0000; (width * height / 8) as usize];
                        for (y, x) in (0..height).cartesian_product(0..width) {
                            let index_8bpp = (y * width) + x;
                            let index_1bpp = ((y * width) + x) / 8;

                            // Set bit to 1 in byte if dithering returned a black pixel
                            if data_8bpp[index_8bpp as usize] > threshold_matrix.look_up(x, y) {
                                data_1bpp[index_1bpp as usize] =
                                    data_1bpp[index_1bpp as usize] | base_two.pow(index_8bpp % 8);
                            };
                        }
                        frame_tx.send(data_1bpp).unwrap();
                    }

                    frame_counter += 1;
                }

                Ok(())
            };

        // Decode packets until the video ended or we cancelled the process
        let mut packet_iter = context_video.packets();
        let mut cancelled: bool = false;

        loop {
            if let Ok(true) = shutdown_rx.try_recv() {
                cancelled = true;
                break;
            }

            match packet_iter.next() {
                Some((stream, packet)) => {
                    if stream.index() == video_stream_index {
                        decoder.send_packet(&packet).unwrap();
                        receive_and_process_decoded_frames(&mut decoder).unwrap();
                    }
                }
                None => {
                    break;
                }
            }
        }

        decoder.send_eof().unwrap();

        if !cancelled {
            receive_and_process_decoded_frames(&mut decoder).unwrap();
        }
    });

    // Spawn the second thread: It will receive the frames and display them on the e-paper device.
    let mut shutdown_rx_panel = shutdown_tx.subscribe();
    let panel_task = task::spawn_blocking(move || {
        // Connect to IT8951 controlled display
        let mut api = API::connect(opt.width, opt.height).unwrap();

        // Get system information
        let system_info = api.get_system_info();
        let image_buffer_base = system_info.image_buffer_base;

        // Calculate byte size of each 1bpp image
        let image_size = (opt.width * opt.height) / 8;

        println!(
            r#"
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

        // Make sure the target sizes fit on the display
        assert!(system_info.width >= opt.width);
        assert!(system_info.height >= opt.height);

        // Set VCOM value
        assert!(opt.vcom < 0.0 && opt.vcom >= -5.0);
        api.set_vcom(opt.vcom).unwrap();

        // Remember register value for later
        let reg = api.get_memory_register_value(0x1800_1138).unwrap();

        // Enable 1bit drawing and image pitch mode
        // 0000 0000 0000 0110 0000 0000 0000 0000
        // |         |     ^^  |         |
        // 113B      113A      1139      1138
        api.set_memory_register_value(0x1800_1138, reg | (1 << 18) | (1 << 17))
            .unwrap();

        // Set bitmap mode color definition (0 - set black(0x00), 1 - set white(0xf0))
        api.set_memory_register_value(0x1800_1250, 0xf0 | (0x00 << 8))
            .unwrap();

        // Set image pitch width
        api.set_memory_register_value(0x1800_124c, (opt.width / 8) / 4)
            .unwrap();

        // Write images to buffer
        let mut frame_counter = 0;
        loop {
            if let Ok(true) = shutdown_rx_panel.try_recv() {
                // Clean up afterwards, by setting screen to white
                api.clear_display().unwrap();
                break;
            }

            if let Ok(frame) = frame_rx.try_recv() {
                // Load images into buffer
                api.set_memory(image_buffer_base, &frame).unwrap();

                // ... so we can finally display the images!
                if frame_counter % opt.ghost == 0 {
                    // Sometimes draw image properly (this is slower) to avoid too much ghosting
                    api.display_image(image_buffer_base, Mode::GL16).unwrap();
                } else {
                    // ... and display the others with a faster mode
                    api.display_image(image_buffer_base, Mode::A2).unwrap();
                }

                frame_counter += 1;
            }
        }
    });

    // Run this until [CTRL] + [C] got pressed or something went wrong
    tokio::select! {
        _ = panel_task => (),
        _ = tokio::signal::ctrl_c() => (),
    }

    println!("\nExit program ..");
    shutdown_tx.send(true).unwrap();

    Ok(())
}