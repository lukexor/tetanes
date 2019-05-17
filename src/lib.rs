// TODO Remove
#![allow(dead_code, unused)]

pub type Result<T> = std::result::Result<T, failure::Error>;

pub mod console;
pub mod disasm;
pub mod ui;
