use log::info;
use std::{env, ffi::OsStr, fs::File, io::BufReader, path::PathBuf};
use structopt::StructOpt;
use tetanes::cartridge::INesHeader;

fn main() {
    env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();

    let opt = Opt::from_args();
    let path = opt
        .path
        .unwrap_or_else(|| env::current_dir().unwrap_or_default());
    if path.is_dir() {
        path.read_dir()
            .unwrap_or_else(|e| panic!("unable read directory {:?}: {}", path, e))
            .filter_map(|f| f.ok())
            .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
            .for_each(|f| print_mapper(&f.path()));
    } else if path.is_file() {
        print_mapper(&path);
    }
}

fn print_mapper(path: &PathBuf) {
    let file = File::open(path).expect("valid path");
    let mut reader = BufReader::new(file);
    let header = INesHeader::load(&mut reader).expect("valid header");
    info!("{:?} - Mapper: {}", path, mapper(header.mapper_num));
}

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(
        help = "The NES ROM or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<PathBuf>,
}

pub fn mapper(mapper_num: u16) -> &'static str {
    match mapper_num {
        0 => "NROM",
        1 => "Sxrom/MMC1",
        2 => "UxROM",
        3 => "CNROM",
        4 => "TxROM/MMC3/MMC6",
        5 => "ExROM/MMC5",
        7 => "AxROM",
        9 => "PxROM",
        _ => "Unsupported Board",
    }
}
