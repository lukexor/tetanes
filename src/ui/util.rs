use crate::Result;
use dirs;
use gl::types::*;
use image::{ImageFormat, RgbaImage};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufReader, Read};
use std::path::Path;

pub fn hash_file<P: AsRef<Path>>(path: &P) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut buf = [0u8; 255];
    file.read_exact(&mut buf)?;
    Ok(format!("{:x}", Sha256::digest(&buf)))
}

pub fn home_dir() -> String {
    dirs::home_dir()
        .unwrap()
        .into_os_string()
        .into_string()
        .unwrap()
}
