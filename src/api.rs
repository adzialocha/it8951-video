use std::fmt;
use std::str;
use std::time::Duration;

use bincode::config::Options;
use rusb::open_device_with_vid_pid;
use serde::{Deserialize, Serialize};

use crate::usb::ScsiOverUsbConnection;

/// SCSI via USB parameter.
const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;
const SCSI_TIMEOUT_MS: u64 = 1000;

/// Customer command.
const CUSTOMER_CMD: u8 = 0xfe;

/// Read from register command.
const READ_REG_CMD: u8 = 0x83;

/// Write to register command.
const WRITE_REG_CMD: u8 = 0x84;

/// PMIC (Power Management Integrated Circuits) command.
const PMIC_CONTROL_CMD: u8 = 0xa3;

// Write to memory in fast mode.
const FAST_WRITE_CMD: u8 = 0xa5;

/// SCSI inquiry command.
const INQUIRY_CMD: [u8; 16] = [
    0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x81, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Command to retreive system information.
const GET_SYS_CMD: [u8; 16] = [
    CUSTOMER_CMD,
    0x00,
    0x38, // Signature[0] of “8951”
    0x39, // Signature[1]
    0x35, // Signature[2]
    0x31, // Signature[3]
    0x80, // Get System information command
    0x00, // Version[0]: 0x00010002
    0x01, // Version[1]
    0x00, // Version[2]
    0x02,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
];

/// Command to display image in an area.
const DPY_AREA_CMD: [u8; 16] = [
    CUSTOMER_CMD,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x94,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
];

/// Command to reset controller to initial state.
const SOFTWARE_RESET_CMD: [u8; 16] = [
    CUSTOMER_CMD,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0xa7,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
];

/// Display modes.
#[repr(u32)]
#[derive(Serialize, PartialEq, Debug)]
pub enum Mode {
    /// The initialization (INIT) mode is used to completely erase the display and leave it in the
    /// white state.
    INIT,

    /// The direct update (DU) is a very fast, non-flashy update.
    ///
    /// This mode supports transitions from any graytone to black or white only. It cannot be used
    /// to update to any graytone other than black or white. The fast update time for this mode
    /// makes it useful for response to touch sensor or pen input or menu selection indictors.
    DU,

    /// The grayscale clearing (GC16) mode is used to update the full display and provide a high
    /// image quality.
    ///
    /// When GC16 is used with Full Display Update the entire display will update as the new image
    /// is written. If a Partial Update command is used the only pixels with changing graytone
    /// values will update. The GC16 mode has 16 unique gray levels.
    GC16,

    /// The GL16 waveform is primarily used to update sparse content on a white background, such as
    /// a page of anti-aliased text, with reduced flash. The GL16 waveform has 16 unique gray
    /// levels.
    GL16,

    /// The GLR16 mode is used in conjunction with an image preprocessing algorithm to update
    /// sparse content on a white background with reduced flash and reduced image artifacts. The
    /// GLR16 mode supports 16 graytones.
    GLR16,

    /// The GLD16 mode is used in conjunction with an image preprocessing algorithm to update
    /// sparse content on a white background with reduced flash and reduced image artifacts.
    ///
    /// It is recommended to be used only with the full display update.
    GLD16,

    /// The A2 mode is a fast, non-flash update mode designed for fast paging turning or simple
    /// black/white animation.
    A2,

    /// The DU4 is a fast update time (similar to DU), non-flashy waveform.
    DU4,

    /// Sometimes the controller reports a weird mode state, this is a placeholder for it.
    UNKNOWN,
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
            Ok(Mode::A2)
        } else if num == 7 {
            Ok(Mode::DU4)
        } else {
            // Sometimes the controller reports some weird numbers, like 154 .., this is why we
            // implement `Deserialize` ourselves.
            Ok(Mode::UNKNOWN)
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// System information about e-paper panel.
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

/// SCSI inquiry command.
pub struct Inquiry {
    pub vendor: String,
    pub product: String,
    pub revision: String,
}

/// An area.
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

/// Talk to the It8951 e-paper display via a USB connection.
pub struct API {
    connection: ScsiOverUsbConnection,
    system_info: SystemInfo,
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
        // Get USB device handle based on vendor ID and product ID. Make sure you have these values
        // whitelisted in your OS configuration aka /etc/udev/rules.d
        let mut device_handle =
            open_device_with_vid_pid(0x48d, 0x08951).expect("Cannot open it8951");
        if let Err(e) = device_handle.set_auto_detach_kernel_driver(true) {
            println!("auto detached failed, error is {}", e);
        }
        device_handle.claim_interface(0)?;

        let mut connection = ScsiOverUsbConnection {
            device_handle,
            endpoint_out: ENDPOINT_OUT,
            endpoint_in: ENDPOINT_IN,
            timeout: Duration::from_millis(SCSI_TIMEOUT_MS),
        };

        // Send first command to device to retreive its system configuration
        let system_info =
            connection.read_command(&GET_SYS_CMD, bincode::options().with_big_endian())?;

        Ok(Self {
            connection,
            system_info,
        })
    }

    /// Return system info about e-paper display.
    pub fn get_system_info(&self) -> &SystemInfo {
        &self.system_info
    }

    /// Send SCSCI inquiry command to receive device information.
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

    /// Read value from memory register of controller.
    pub fn get_memory_register_value(&mut self, address: u32) -> rusb::Result<u32> {
        let address_8 = address.to_be_bytes();

        let command = [
            CUSTOMER_CMD,
            0x00,
            address_8[0],
            address_8[1],
            address_8[2],
            address_8[3],
            READ_REG_CMD,
            0x00, // Length[0]
            0x04, // Length[1]: 4
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        let result: u32 = self.connection.read_command(&command, bincode::options())?;

        Ok(result)
    }

    /// Set memory register value of controller.
    pub fn set_memory_register_value(&mut self, address: u32, data: u32) -> rusb::Result<()> {
        let address_8 = address.to_be_bytes();

        let command = [
            CUSTOMER_CMD,
            0x00,
            address_8[0],
            address_8[1],
            address_8[2],
            address_8[3],
            WRITE_REG_CMD,
            0x00, // Length[0]
            0x04, // Length[1]: 4 bytes
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        self.connection
            .write_command_raw(&command, &data.to_be_bytes())
    }

    /// Set VCOM value of controller.
    pub fn set_vcom(&mut self, vcom: u16) -> rusb::Result<()> {
        let [vcom_h, vcom_l] = vcom.to_be_bytes();

        let data = [
            CUSTOMER_CMD,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            PMIC_CONTROL_CMD,
            vcom_h, // Set VCom Value [15:8]
            vcom_l, // Set VCom Value [7:0]
            0x01,   // Do Set VCom? (0 – no, 1 – yes)
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        self.connection.write_command_raw(&data, &[])
    }

    /// Reset device state to default.
    pub fn reset(&mut self) -> rusb::Result<SystemInfo> {
        self.connection
            .read_command(&SOFTWARE_RESET_CMD, bincode::options().with_big_endian())
    }

    /// Write any data to memory using fast-write mode.
    pub fn fast_write_to_memory(&mut self, address: u32, data: &[u8]) -> rusb::Result<()> {
        let address_8 = address.to_be_bytes();
        let data_len_8 = (data.len() as u16).to_be_bytes();

        let command = [
            CUSTOMER_CMD,
            0x00,
            address_8[0],
            address_8[1],
            address_8[2],
            address_8[3],
            FAST_WRITE_CMD,
            data_len_8[0],
            data_len_8[1],
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
        ];

        self.connection.write_command_raw(&command, &data)
    }

    pub fn display_image(&mut self, address: u32, mode: Mode) -> rusb::Result<()> {
        let system_info = self.get_system_info();

        self.connection.write_command(
            &DPY_AREA_CMD,
            DisplayArea {
                address,
                display_mode: mode,
                x: 0,
                y: 0,
                w: system_info.width,
                h: system_info.height,
                wait_ready: 1,
            },
            &[],
            bincode::options().with_big_endian(),
        )?;

        Ok(())
    }
}
