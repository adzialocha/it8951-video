use std::fmt;
use std::mem;
use std::str;
use std::time::Duration;

use bincode::config::Options;
use rusb::open_device_with_vid_pid;
use rusb::{DeviceHandle, GlobalContext};
use serde::{Deserialize, Serialize};

use crate::usb::ScsiOverUsbConnection;

/// SCSI via USB parameter.
const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;

/// Maximum transfer size is 60k bytes for IT8951 USB.
const MAX_TRANSFER: usize = 60 * 1024;

/// SCSI inquiry command.
const INQUIRY_CMD: [u8; 16] = [0x12, 0, 0, 0, 0, 0, 0x81, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Get system information.
const GET_SYS_CMD: [u8; 16] = [
    0xfe, 0x00, 0x38, 0x39, 0x35, 0x31, 0x80, 0, 0x01, 0, 0x02, 0, 0, 0, 0, 0,
];

/// Host Load Image Area function.
const LD_IMAGE_AREA_CMD: [u8; 16] = [
    0xfe, // Customer command
    0x00, 0x00, 0x00, 0x00, 0x00, 0xa2, // Load image area command
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

///
const DPY_AREA_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0x94, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

///
const SOFTWARE_RESET_CMD: [u8; 16] = [
    0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa7, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Display mode.
#[repr(u32)]
#[derive(Serialize, PartialEq, Debug)]
pub enum Mode {
    INIT,
    DU,
    GC16,
    GL16,
    GLR16,
    GLD16,
    DU4,
    A2,
    UNKNOWN, // Don't know what mode it is, but Waveshare 7.8inch e-Paper HAT reports this mode
}

impl<'de> Deserialize<'de> for Mode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let num: u32 = Deserialize::deserialize(deserializer)?;

        if num == 0 {
            Ok(Mode::INIT)
        } else if num == 1 {
            Ok(Mode::DU)
        } else if num == 2 {
            Ok(Mode::GC16)
        } else if num == 3 {
            Ok(Mode::GL16)
        } else if num == 4 {
            Ok(Mode::GLR16)
        } else if num == 5 {
            Ok(Mode::GLD16)
        } else if num == 6 {
            Ok(Mode::DU4)
        } else if num == 7 {
            Ok(Mode::A2)
        } else {
            Ok(Mode::UNKNOWN)
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// System information about epaper panel.
#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct SystemInfo {
    standard_cmd_no: u32,
    extended_cmd_no: u32,
    signature: u32,
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub update_buf_base: u32,
    pub image_buffer_base: u32,
    temperature_no: u32,
    pub mode: Mode,
    frame_count: [u32; 8],
    num_img_buf: u32,
    reserved: [u32; 9],
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct CInquiry {
    ignore_start: [u8; 8],
    pub vendor: [u8; 8],
    pub product: [u8; 16],
    pub revision: [u8; 4],
    ignore_end: [u8; 4],
}

/// Inquiry.
///
/// If it works, it's going to be uninteresting:
/// ```
/// vendor: Generic
/// product: Storage RamDisc
/// revision: 1.00
// ```
pub struct Inquiry {
    pub vendor: String,
    pub product: String,
    pub revision: String,
}

/// An area
#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Area {
    address: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[repr(C)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct DisplayArea {
    address: u32,
    display_mode: Mode,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    wait_ready: u32,
}

fn open_it8951() -> Option<DeviceHandle<GlobalContext>> {
    open_device_with_vid_pid(0x48d, 0x8951)
}

/// Talk to the It8951 e-paper display via a USB connection
pub struct API {
    connection: ScsiOverUsbConnection,
    system_info: Option<SystemInfo>,
}

impl Drop for API {
    fn drop(&mut self) {
        self.connection
            .device_handle
            .release_interface(0)
            .expect("release failed");
    }
}

impl API {
    /// Establish a connection to the e-paper display via the USB port.
    pub fn connect() -> rusb::Result<Self> {
        let mut device_handle = open_it8951().expect("Cannot open it8951");
        if let Err(e) = device_handle.set_auto_detach_kernel_driver(true) {
            println!("auto detached failed, error is {}", e);
        }
        device_handle.claim_interface(0).expect("claim failed");

        let mut result = Self {
            connection: ScsiOverUsbConnection {
                device_handle,
                endpoint_out: ENDPOINT_OUT,
                endpoint_in: ENDPOINT_IN,
                timeout: Duration::from_millis(1000),
            },
            system_info: None,
        };
        let system_info = result.get_sys()?;
        result.system_info = Some(system_info);

        Ok(result)
    }

    /// Return system info about e-paper display.
    pub fn get_system_info(&self) -> &SystemInfo {
        self.system_info.as_ref().unwrap()
    }

    /// Make an inquiry.
    pub fn inquiry(&mut self) -> rusb::Result<Inquiry> {
        let c_inquiry: CInquiry = self
            .connection
            .read_command(&INQUIRY_CMD, bincode::options())?;
        Ok(Inquiry {
            vendor: str::from_utf8(&c_inquiry.vendor).unwrap().to_string(),
            product: str::from_utf8(&c_inquiry.product).unwrap().to_string(),
            revision: str::from_utf8(&c_inquiry.revision).unwrap().to_string(),
        })
    }

    fn get_sys(&mut self) -> rusb::Result<SystemInfo> {
        self.connection
            .read_command(&GET_SYS_CMD, bincode::options().with_big_endian())
    }

    pub fn reset(&mut self) -> rusb::Result<SystemInfo> {
        self.connection
            .read_command(&SOFTWARE_RESET_CMD, bincode::options().with_big_endian())
    }

    /// Execute LD_IMG_AREA command.
    ///
    /// The sent image will be collected by IT8951 whatever Host sends partial or full image.
    fn ld_image_area(&mut self, area: Area, data: &[u8]) -> rusb::Result<()> {
        self.connection.write_command(
            &LD_IMAGE_AREA_CMD,
            area,
            data,
            bincode::options().with_big_endian(),
        )
    }

    fn dpy_area(&mut self, display_area: DisplayArea) -> rusb::Result<()> {
        self.connection.write_command(
            &DPY_AREA_CMD,
            display_area,
            &[],
            bincode::options().with_big_endian(),
        )
    }

    pub fn preload_image(&mut self, frame: Vec<u8>, address: u32) -> rusb::Result<()> {
        let system_info = self.get_system_info();

        // Load image into buffer
        let data = frame.as_slice();
        let w: usize = system_info.width as usize;
        let h: usize = system_info.height as usize;
        let size = w * h;

        // We send the image in bands of MAX_TRANSFER
        let mut i: usize = 0;
        let mut row_height = (MAX_TRANSFER - mem::size_of::<Area>()) / w;

        while i < size {
            // We don't want to go beyond the end with the last band
            if (i / w) + row_height > h {
                row_height = h - (i / w);
            }

            self.ld_image_area(
                Area {
                    address,
                    x: 0,
                    y: (i / w) as u32,
                    w: w as u32,
                    h: row_height as u32,
                },
                &data[i..i + w * row_height],
            )?;

            i += row_height * w;
        }

        Ok(())
    }

    pub fn display_image(&mut self, mode: Mode, address: u32) -> rusb::Result<()> {
        let system_info = self.get_system_info();

        self.dpy_area(DisplayArea {
            address,
            display_mode: mode,
            x: 0,
            y: 0,
            w: system_info.width,
            h: system_info.height,
            wait_ready: 1,
        })?;

        Ok(())
    }
}
