use crate::{
    driver::{Driver, DriverOpts},
    event::PixEvent,
    Result,
};

pub(super) struct WasmDriver {}

impl WasmDriver {
    pub(super) fn new(opts: DriverOpts) -> Self {
        Self {}
    }
}

impl Driver for WasmDriver {}
