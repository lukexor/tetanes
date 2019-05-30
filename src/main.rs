//! Usage: nes [rom_file | rom_directory]
//!
//! 1. If a rom file is provided, that rom is loaded
//! 2. If a directory is provided, `.nes` files are searched for in that directory
//! 3. If no arguments are provided, the current directory is searched for rom files ending in
//!    `.nes`
//!
//! In the case of 2 and 3, if valid NES rom files are found, a menu screen is displayed to select
//! which rom to run. If there are any errors related to invalid files, directories, or
//! permissions, the program will print an error and exit.

use failure::{format_err, Error};
use rustynes::ui::UI;
use rustynes::util::Result;
use std::{env, path::PathBuf};
use structopt::StructOpt;

/// Command-Line Options
#[derive(StructOpt, Debug)]
#[structopt(
    name = "nes",
    about = "An NES emulator written in Rust.",
    version = "0.1.0",
    author = "Luke Petherbridge <me@lukeworks.tech>"
)]
struct Opt {
    #[structopt(short = "d", long = "debug", help = "Debug")]
    debug: bool,
    #[structopt(short = "f", long = "fullscreen", help = "Fullscreen")]
    fullscreen: bool,
    #[structopt(short = "l", long = "load", help = "Load Save State")]
    load: Option<u8>,
    #[structopt(
        short = "s",
        long = "scale",
        default_value = "3",
        help = "Window scale"
    )]
    scale: u32,
    #[structopt(
        parse(from_os_str),
        help = "The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<PathBuf>,
}

fn main() {
    let opt = Opt::from_args();
    let roms = find_roms(opt.path).unwrap_or_else(|e| err_exit(e));
    let mut ui = UI::init(roms, opt.debug).unwrap_or_else(|e| err_exit(e));
    ui.run(opt.fullscreen, opt.scale, opt.load)
        .unwrap_or_else(|e| err_exit(e));
}

// Searches for valid NES rom files ending in `.nes`
//
// If rom_path is a `.nes` file, uses that
// If no arg[1], searches current directory for `.nes` files
fn find_roms(path: Option<PathBuf>) -> Result<Vec<PathBuf>> {
    use std::ffi::OsStr;

    let rom_path = path.unwrap_or_else(|| env::current_dir().unwrap_or_default());
    let mut roms = Vec::new();
    if rom_path.is_dir() {
        rom_path
            .read_dir()
            .map_err(|e| format_err!("unable to read directory {:?}: {}", rom_path, e))?
            .filter_map(|f| f.ok())
            .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
            .for_each(|f| roms.push(f.path()));
    } else if rom_path.is_file() {
        roms.push(rom_path.to_path_buf());
    } else {
        Err(format_err!("invalid path: {:?}", rom_path))?;
    }
    if roms.is_empty() {
        Err(format_err!("no rom files found or specified"))?;
    }
    Ok(roms)
}

fn err_exit(err: Error) -> ! {
    eprintln!("Err: {}", err.to_string());
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_roms() {
        let rom_tests = &[
            // (Test name, Path, Error)
            // CWD with no `.nes` files
            (
                "CWD with no nes files",
                None,
                "no rom files found or specified",
            ),
            // Directory with no `.nes` files
            (
                "Dir with no nes files",
                Some("src/"),
                "no rom files found or specified",
            ),
            (
                "invalid directory",
                Some("invalid/"),
                "invalid path: \"invalid/\"",
            ),
        ];
        for test in rom_tests {
            let path = if let Some(p) = test.1 {
                Some(PathBuf::from(p))
            } else {
                None
            };
            let roms = find_roms(path);
            assert!(roms.is_err(), "invalid path {}", test.0);
            assert_eq!(
                roms.err().unwrap().to_string(),
                test.2,
                "error matches {}",
                test.0
            );
        }
    }
}
