pub mod cartridge;
pub mod console;
pub mod disasm;
pub mod input;
pub mod mapper;
pub mod memory;
pub mod ui;

pub mod util {
    use dirs;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    pub type Result<T> = std::result::Result<T, failure::Error>;

    const SAVE_DIR: &str = ".rustynes";
    const DAT_SUFFIX: &str = "dat";

    pub fn save_path<P: AsRef<Path>>(path: &P) -> Result<PathBuf> {
        let filehash = hash_file(path)?;
        let mut path = home_dir().unwrap_or_else(|| PathBuf::from("./"));
        path.push(SAVE_DIR);
        path.push("sram");
        path.push(filehash);
        path.set_extension(DAT_SUFFIX);
        Ok(path)
    }

    pub fn hash_file<P: AsRef<Path>>(path: &P) -> Result<String> {
        let mut file = fs::File::open(path)?;
        let mut buf = [0u8; 255];
        file.read_exact(&mut buf)?;
        Ok(format!("{:x}", Sha256::digest(&buf)))
    }

    pub fn home_dir() -> Option<PathBuf> {
        dirs::home_dir().and_then(|d| Some(d.to_path_buf()))
    }
}
