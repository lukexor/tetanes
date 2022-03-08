use log::info;
use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use tetanes::{cart::NesHeader, NesResult};

fn main() -> NesResult<()> {
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
        let mut boards: Vec<String> = paths
            .iter()
            .map(get_mapper)
            .filter_map(Result::ok)
            .filter(|b| match &board {
                Some(board) => b.to_lowercase().contains(board),
                None => true,
            })
            .collect();
        boards.sort();
        boards.iter().for_each(|board| info!("{}", board));
    } else if path.is_file() {
        info!("{}", get_mapper(&path)?);
    }
    Ok(())
}

fn get_mapper<P: AsRef<Path>>(path: P) -> NesResult<String> {
    let path = path.as_ref();
    let file = File::open(path).expect("valid path");
    let mut reader = BufReader::new(file);
    let board = NesHeader::load(&mut reader)?.mapper_board();
    Ok(format!("{:<30} {:?}", board, path))
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
