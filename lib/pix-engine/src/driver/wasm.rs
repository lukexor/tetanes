use crate::{
    driver::{Driver, DriverOpts},
    PixEngineResult,
};
use wasm_bindgen::prelude::*;

pub(crate) struct WasmDriver {}

impl WasmDriver {
    pub(crate) fn new(_opts: DriverOpts) -> PixEngineResult<Self> {
        Ok(Self {})
    }
}

impl Driver for WasmDriver {}
