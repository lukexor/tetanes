// TODO remove this when finished
#![allow(dead_code, unused_imports, unused_variables)]
use std::{error, fmt};

pub mod event;
pub mod pixel;

mod audio;
mod driver;
mod engine;
mod state;

pub use engine::PixEngine;
pub use image::{DynamicImage, GenericImage, GenericImageView, Rgb, Rgba};
pub use pixel::Sprite;
pub use state::{draw, transform, AlphaMode, State, StateData};

pub type PixEngineResult<T> = std::result::Result<T, PixEngineErr>;

pub struct PixEngineErr {
    description: String,
}

impl PixEngineErr {
    pub fn new(desc: &str) -> Self {
        Self {
            description: desc.to_string(),
        }
    }
}

impl fmt::Display for PixEngineErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description)
    }
}

impl fmt::Debug for PixEngineErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{{ err: {}, file: {}, line: {} }}",
            self.description,
            file!(),
            line!(),
        )
    }
}

impl error::Error for PixEngineErr {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

impl From<std::io::Error> for PixEngineErr {
    fn from(err: std::io::Error) -> Self {
        Self {
            description: err.to_string(),
        }
    }
}
