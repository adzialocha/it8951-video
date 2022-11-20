# it8951-video

Play videos on IT8951-controlled e-paper displays via USB. This has been tested with a [Waveshare 7.8inch e-Paper HAT](https://www.waveshare.com/wiki/7.8inch_e-Paper_HAT) display.

![Example Video](example.gif)

## Design

This runs fairly smooth considering it is a e-Paper display (~5 fps) which has not been optimized for video usage. The following "tricks" have been used:

* Convert every video frame into an image which only contains black or white pixels (via dithering)
* Pack pixel information for every frame into 1bit (black = 0, white = 1) and store it in a file so it can be played later (don't do this on-the-fly as the dithering process takes too much time)
* Activate undocumented 1bpp (1 bit per pixel) and "pitch" mode on IT8951 by flipping the bits in the `0x1800_1138` register before displaying
* Store frame data via "fast write" (fw) `0xa5` command in memory
* Since the data is smaller now (322944 bytes) than grayscale images we can store up to 8 frames in the image buffer (which usually only has space for one image)
* Always write in `A2` mode since it is fast and does not cause any flashing with b/w-only data. Use `GL16` mode sometimes, just to make sure the ghosting does not minder the quality too much

## Requirements

* ffmpeg
* Rust

## Preparation

In order to make this work you need to create a udev rule that gives users permission to talk to this device. To this end add a file 60-it8951.rules to the `/etc/udev/rules.d` directory with the following contents:

```
SUBSYSTEM=="usb", ATTRS{idVendor}=="048d", MODE="0666"
```

This gives applications access to talk to devices by vendor "048d", which is the IT8951. You can then restart your system, or by write this to trigger without reboot:

```
udevadm control --reload-rules && udevadm trigger
```

## Usage

```
Play videos on IT8951-controlled e-paper displays

USAGE:
    it8951-video [OPTIONS] <input>

FLAGS:
        --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -v, --vcom <vcom>        VCOM value [default: -1.58]
    -g, --ghost <ghost>      Paint in GL16 mode every nth frame [default: 32]
    -t, --take <take>        Only take every nth frame from video [default: 5]
    -w, --width <width>      Width of video on display [default: 1856]
    -h, --height <height>    Height of video on display [default: 1392]

ARGS:
    <input>    Video file which will be displayed
```

## Credits

* [@bspth](https://github.com/bspth) for finding almost every hack which made this work at all
* https://github.com/faassen/rust-it8951 for the SCSI over USB communication with IT8951
