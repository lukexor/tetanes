use log::info;
use std::{env, ffi::OsStr, fs::File, io::BufReader, path::Path, path::PathBuf};
use structopt::StructOpt;
use tetanes::cartridge::INesHeader;

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
            .filter_map(|f| f.ok())
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

fn print_mapper(path: &Path, board: Option<&String>) {
    let file = File::open(path).expect("valid path");
    let mut reader = BufReader::new(file);
    if let Ok(header) = INesHeader::load(&mut reader) {
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
struct Opt {
    #[structopt(
        help = "The NES ROM or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<PathBuf>,
    #[structopt(help = "The NES Mapper Board to filter by.")]
    board: Option<String>,
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
        11 => "COLORDREAMS",
        69 => "JxROM/BTR",
        71 => "CAMERICA",
        206 => "DxROM",
        _ => "Unknown Board",
    }
}
