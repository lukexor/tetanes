// TODO Remove
#![allow(dead_code, unused)]

pub type Result<T> = std::result::Result<T, failure::Error>;

mod console;
mod disasm;
pub mod ui;
