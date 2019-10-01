// TODO remove this when finished
#![allow(dead_code, unused_imports, unused_variables)]
use std::{error, fmt};

pub mod event;
pub mod pixel;

mod driver;
mod engine;
mod state;

pub use engine::PixEngine;
pub use pixel::{Pixel, Sprite};
pub use state::{transform, State, StateData};

type Result<T> = std::result::Result<T, PixEngineErr>;

#[derive(Debug)]
pub struct PixEngineErr {
    description: String,
}

impl PixEngineErr {
    fn new(description: String) -> Self {
        Self { description }
    }
}

impl fmt::Display for PixEngineErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description)
    }
}

impl error::Error for PixEngineErr {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

impl From<std::io::Error> for PixEngineErr {
    fn from(err: std::io::Error) -> PixEngineErr {
        PixEngineErr {
            description: err.to_string(),
        }
    }
}
