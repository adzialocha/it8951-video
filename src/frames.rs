use serde::{Deserialize, Serialize};

pub type RawFrame = Vec<u8>;

#[derive(Debug, Serialize, Deserialize)]
pub struct RawFrames(u32, u32, Vec<RawFrame>);

impl RawFrames {
    pub fn new(width: u32, height: u32, data: Vec<RawFrame>) -> Self {
        Self(width, height, data)
    }

    pub fn get(&self, index: usize) -> Vec<u8> {
        self.2.get(index).unwrap().to_owned()
    }

    pub fn width(&self) -> u32 {
        self.0
    }

    pub fn height(&self) -> u32 {
        self.1
    }
}
