use log::info;
use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use tetanes::cartridge::NesHeader;

fn main() {
    env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();

    let opt = Opt::from_args();
    let path = opt
        .path
        .unwrap_or_else(|| env::current_dir().unwrap_or_default());
    let board = opt.board.map(|b| b.to_lowercase());
    if path.is_dir() {
        let paths: Vec<PathBuf> = path
            .read_dir()
            .unwrap_or_else(|e| panic!("unable read directory {:?}: {}", path, e))
            .filter_map(Result::ok)
            .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
            .map(|f| f.path())
            .collect();
        for p in paths {
            print_mapper(&p, board.as_ref());
        }
    } else if path.is_file() {
        print_mapper(&path, board.as_ref());
    }
}

fn print_mapper<P: AsRef<Path>>(path: P, board: Option<&String>) {
    let path = path.as_ref();
    let file = File::open(path).expect("valid path");
    let mut reader = BufReader::new(file);
    if let Ok(header) = NesHeader::load(&mut reader) {
        if board.is_none()
            || mapper(header.mapper_num)
                .to_lowercase()
                .contains(board.unwrap())
        {
            info!(
                "{:?} - Mapper: {}, Board: {}",
                path,
                header.mapper_num,
                mapper(header.mapper_num)
            );
        }
    }
}

#[derive(StructOpt, Debug)]
#[must_use]
struct Opt {
    #[structopt(
        help = "The NES ROM or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<PathBuf>,
    #[structopt(help = "The NES Mapper Board to filter by.")]
    board: Option<String>,
}

#[must_use]
pub const fn mapper(mapper_num: u16) -> &'static str {
    match mapper_num {
        0 => "Mapper 000 - NROM",
        1 => "Mapper 001 - SxROM/MMC1",
        2 => "Mapper 002 - UxROM",
        3 => "Mapper 003 - CNROM",
        4 => "Mapper 004 - TxROM/MMC3/MMC6",
        5 => "Mapper 005 - ExROM/MMC5",
        7 => "Mapper 007 - AxROM",
        9 => "Mapper 009 - PxROM",
        11 => "Mapper 011 - COLORDREAMS",
        69 => "Mapper 069 - JxROM/BTR",
        71 => "Mapper 071 - UxROM/CAMERICA",
        155 => "Mapper 155 - SxROM/MMC1A",
        206 => "Mapper 206 - DxROM",
        _ => "Unknown Board",
    }
}
