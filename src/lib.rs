// TODO Remove
#![allow(dead_code, unused)]

pub type Result<T> = std::result::Result<T, failure::Error>;

pub mod cartridge;
pub mod console;
pub mod disasm;
pub mod input;
pub mod mapper;
pub mod memory;
pub mod ui;
