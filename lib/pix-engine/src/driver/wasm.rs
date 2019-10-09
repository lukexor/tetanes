use crate::{
    driver::{Driver, DriverOpts},
    event::PixEvent,
    Result,
};
use image::DynamicImage;

pub(super) struct WasmDriver {}

impl WasmDriver {
    pub(super) fn new(opts: DriverOpts) -> Self {
        Self {}
    }
}

impl Driver for WasmDriver {}
