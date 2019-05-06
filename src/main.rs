//! Usage: nes [rom_file | rom_directory]
//!
//! 1. If no arguments are passed, the current directory is searched for rom files ending in .nes
//! 2. If a directory is passed, .nes files are searched for in that directory
//! 3. If a rom file is passed, that rom is loaded
//!
//! In the case of 1 and 2, if rom files are found, a menu screen is displayed to select which rom
//! to run. If there are any errors related to invalid files, directories, or permissions, the
//! program will print the error and exit.

// use nes::ui::UI;
// use std::{env, error::Error, path::PathBuf};

fn main() {
    // let roms = find_roms().unwrap_or_else(|e| err_exit(e));
    // let mut ui = UI::new(roms).unwrap_or_else(|e| err_exit(e));
    // ui.run();
}

// /// TODO: Document
// fn find_roms() -> Result<Vec<PathBuf>, Box<Error>> {
//     let mut args = env::args().skip(1);
//     let rom_path = match args.next() {
//         Some(path) => PathBuf::from(path),
//         None => env::current_dir().unwrap_or_default(),
//     };
//     let mut roms = Vec::new();
//     if rom_path.is_dir() {
//         match rom_path.read_dir() {
//             Ok(entries) => {
//                 entries
//                     .filter_map(Result::ok)
//                     .filter(|f| {
//                         if let Some(e) = f.path().extension() {
//                             e == "nes"
//                         } else {
//                             false
//                         }
//                     })
//                     .for_each(|f| roms.push(f.path()));
//             }
//             Err(err) => {
//                 return Err(format!(
//                     "unable to read directory `{}`: {}",
//                     rom_path.to_string_lossy(),
//                     err
//                 )
//                 .into());
//             }
//         }
//     } else if rom_path.is_file() {
//         roms.push(rom_path);
//     }
//     Ok(roms)
// }

// fn err_exit(err: Box<Error>) -> ! {
//     eprintln!("{}", err.to_string());
//     std::process::exit(1);
// }
