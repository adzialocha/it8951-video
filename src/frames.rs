use serde::{Deserialize, Serialize};

type RawFramesData = Vec<Vec<Vec<u8>>>;

#[derive(Debug, Serialize, Deserialize)]
pub struct RawFrames(u32, u32, RawFramesData);

impl RawFrames {
    pub fn new(width: u32, height: u32, data: RawFramesData) -> Self {
        Self(width, height, data)
    }

    pub fn width(&self) -> u32 {
        self.0
    }

    pub fn height(&self) -> u32 {
        self.1
    }
}
