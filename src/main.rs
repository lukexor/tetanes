//! Usage: nes [rom_file | rom_directory]
//!
//! 1. If no arguments are passed, the current directory is searched for rom files ending in .nes
//! 2. If a directory is passed, .nes files are searched for in that directory
//! 3. If a rom file is passed, that rom is loaded
//!
//! In the case of 1 and 2, if rom files are found, a menu screen is displayed to select which rom
//! to run. If there are any errors related to invalid files, directories, or permissions, the
//! program will print the error and exit.

use nes::core::cartridge::Cartridge;
use nes::core::console::Console;
// use nes::ui;
// use std::env;
// use std::error::Error;
// use std::path::PathBuf;

fn main() {
    // // Find rom(s) to run
    // let roms = find_roms().unwrap_or_else(|err| {
    //     eprintln!("{}", err.to_string());
    //     std::process::exit(1);
    // });
    // // Run main loop
    // std::process::exit(match ui::run(roms) {
    //     Ok(_) => 0,
    //     Err(err) => {
    //         eprintln!("{}", err.to_string());
    //         1
    //     }
    // });

    let rom = "roms/Zelda II - The Adventure of Link (USA).nes";
    let cartridge = Cartridge::new(rom).expect("valid cartridge");
    let mut console = Console::new(cartridge).expect("valid console");
    for _ in 0..10 {
        console.step();
    }
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
