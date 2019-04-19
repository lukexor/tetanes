use dirs;
use sha2::{Digest, Sha256};
use std::{error::Error, fs::File, io::Read, path::PathBuf};

pub fn hash_file(path: &PathBuf) -> Result<String, Box<Error>> {
    let mut file = File::open(path)?;
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
