use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ffmpeg_next::format::{input, Pixel};
use ffmpeg_next::media::Type;
use ffmpeg_next::software::scaling::{context::Context, flag::Flags};
use ffmpeg_next::util::frame::video::Video;
use image::imageops::BiLevel;
use image::{GrayImage, Luma};
use itertools::Itertools;
use pbr::ProgressBar;
use structopt::StructOpt;

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

    /// Output video width.
    #[structopt(short = "w", long = "width", default_value = "1856")]
    width: u32,

    /// Output video height.
    #[structopt(short = "h", long = "height", default_value = "1392")]
    height: u32,
}

fn extract_video_frames<P>(
    file_path: &P,
    target_width: u32,
    target_height: u32,
) -> Result<RawFrames>
where
    P: AsRef<Path>,
{
    let mut frames = Vec::new();

    // Find video stream in file
    let mut context_video = input(file_path)?;
    let input = context_video
        .streams()
        .best(Type::Video)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;
    let video_stream_index = input.index();

    // Prepare video decoder
    let context_decoder =
        ffmpeg_next::codec::context::Context::from_parameters(input.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    // Prepare scaler which resizes the frames and converts them to grayscale
    let mut scaler = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::GRAY8,
        target_width,
        target_height,
        Flags::BILINEAR,
    )?;

    let mut pb = ProgressBar::new(input.frames() as u64);
    pb.format("╢▌▌░╟");
    let mut frame_index = 0;

    let mut receive_and_process_decoded_frames =
        |decoder: &mut ffmpeg_next::decoder::Video| -> Result<(), ffmpeg_next::Error> {
            let mut decoded = Video::empty();

            while decoder.receive_frame(&mut decoded).is_ok() {
                // Decode next frame
                let mut frame = Video::empty();
                scaler.run(&decoded, &mut frame)?;

                // Encode as grayscale image
                let frame_data = frame.data(0);
                let mut img = GrayImage::from_fn(frame.width(), frame.height(), |x, y| {
                    let luma = frame_data[(y as usize * frame.width() as usize) + x as usize];
                    Luma::from([luma])
                });

                // Apply dithering filter
                image::imageops::colorops::dither(&mut img, &BiLevel);

                // Convert to raw format, representing black (0) or white (1) pixels in an array
                let iter = img.enumerate_pixels();
                let mut data: Vec<u8> = Vec::new();
                let base: u8 = 2;

                for chunk in &iter.chunks(8) {
                    let mut byte: u8 = 0b0000_0000;

                    chunk.enumerate().for_each(|(index, (_, _, luma))| {
                        if luma.0[0] == 255 {
                            byte |= base.pow(index as u32);
                        }
                    });

                    data.push(byte);
                }

                frames.push(data);
                frame_index += 1;
                pb.inc();
            }

            Ok(())
        };

    for (stream, packet) in context_video.packets() {
        if stream.index() == video_stream_index {
            decoder.send_packet(&packet)?;
            receive_and_process_decoded_frames(&mut decoder)?;
        }
    }

    decoder.send_eof()?;
    receive_and_process_decoded_frames(&mut decoder)?;

    Ok(RawFrames::new(target_width, target_height, frames))
}

fn write_raw_frames(file_path: PathBuf, frames: &RawFrames) -> Result<()> {
    let encoded: Vec<u8> = bincode::serialize(&frames)?;
    fs::write(file_path, encoded)?;
    Ok(())
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    println!(
        r#"► Convert video for e-paper

Video Dimensions: {}x{}
        "#,
        opt.width, opt.height
    );

    let frames = extract_video_frames(&opt.video, opt.width, opt.height)?;
    write_raw_frames(opt.output, &frames)?;
    Ok(())
}
