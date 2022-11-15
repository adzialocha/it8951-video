use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use image::imageops::BiLevel;
use image::{Rgb, RgbImage};
use itertools::Itertools;
use structopt::StructOpt;
use video_rs::{Decoder, Locator};

use it8951_video::RawFrames;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "it8951-video-convert",
    about = "Convert videos to raw frames to prepare them being displayed on e-paper screen"
)]
struct Opt {
    /// Input video file.
    #[structopt(parse(from_os_str))]
    video: PathBuf,

    /// Output raw data file.
    #[structopt(parse(from_os_str))]
    output: PathBuf,

    #[structopt(short = "w", long = "width", default_value = "1872")]
    width: u32,

    #[structopt(short = "h", long = "height", default_value = "1404")]
    height: u32,
}

fn extract_video_frames(
    file_path: PathBuf,
    target_width: u32,
    target_height: u32,
) -> Result<RawFrames> {
    let source = Locator::from(file_path);
    let mut decoder = Decoder::new(&source)?;

    let data = decoder
        .decode_iter()
        .take_while(Result::is_ok)
        .take(5)
        .map(|frame| frame.unwrap())
        .enumerate()
        .map(|(index, (_, raw_frame))| {
            println!("Convert frame {index}");
            let size = raw_frame.shape();
            let width = size[0] as u32;
            let height = size[1] as u32;

            // Load frame data into image buffer
            let image = RgbImage::from_fn(height, width, |x, y| {
                let rgb = raw_frame
                    .slice(ndarray::s![y as usize, x as usize, ..])
                    .to_slice()
                    .unwrap();
                Rgb([rgb[0], rgb[1], rgb[2]])
            });

            // Resize image to make it cover whole display
            let resized_image = image::imageops::resize(
                &image,
                target_width,
                target_height,
                image::imageops::FilterType::Nearest,
            );

            // Apply b/w dithering on image buffer
            let mut grayscale_image = image::imageops::grayscale(&resized_image);
            image::imageops::colorops::dither(&mut grayscale_image, &BiLevel);

            // Convert to raw format, representing black (0) or white (1) pixels in an array
            let iter = grayscale_image.enumerate_pixels();
            let mut data: Vec<u8> = Vec::new();
            let base: u8 = 2;

            for chunk in &iter.chunks(8) {
                let mut byte: u8 = 0b0000_0000;

                chunk.enumerate().for_each(|(index, (_, _, luma))| {
                    if luma.0[0] == 255 {
                        byte |= base.pow(7 - index as u32)
                    }
                });

                data.push(byte);
            }

            data
        })
        .collect();

    Ok(RawFrames::new(target_width, target_height, data))
}

fn write_raw_frames(file_path: PathBuf, frames: &RawFrames) -> Result<()> {
    let encoded: Vec<u8> = bincode::serialize(&frames)?;
    fs::write(file_path, encoded)?;
    Ok(())
}

fn main() -> Result<()> {
    let opt = Opt::from_args();
    let frames = extract_video_frames(opt.video.clone(), opt.width, opt.height)?;
    write_raw_frames(opt.output, &frames)?;
    Ok(())
}
