use std::slice::Iter;

use serde::{Deserialize, Serialize};

pub type RawFrame = Vec<u8>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawFrames(u32, u32, Vec<RawFrame>);

impl RawFrames {
    pub fn new(width: u32, height: u32, data: Vec<RawFrame>) -> Self {
        Self(width, height, data)
    }

    pub fn len(&self) -> usize {
        self.2.len()
    }

    pub fn iter(&self) -> Iter<RawFrame> {
        self.2.iter()
    }

    pub fn get(&self, index: usize) -> Option<&Vec<u8>> {
        self.2.get(index)
    }

    pub fn width(&self) -> u32 {
        self.0
    }

    pub fn height(&self) -> u32 {
        self.1
    }
}
