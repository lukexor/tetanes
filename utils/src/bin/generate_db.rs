use anyhow::Context;
use std::{
    collections::hash_map::DefaultHasher,
    env,
    ffi::OsStr,
    fs::File,
    hash::{Hash, Hasher},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use tetanes::{cart::Cart, mem::RamState, NesResult};

const GAME_DB: &str = "config/game_database.txt";

fn main() -> NesResult<()> {
    let opt = Opt::from_args();
    let path = opt
        .path
        .unwrap_or_else(|| env::current_dir().unwrap_or_default());
    let header = "# Fields: Hash, Region, Board, PCB, Chip, Mapper, PrgRomSize, ChrRomSize, ChrRamSize, PrgRamSize, Battery, Mirroring, SubMapper, Title";
    if path.is_dir() {
        let mut db_file =
            BufWriter::new(File::create(GAME_DB).context("failed to open game_database.txt")?);
        let paths: Vec<PathBuf> = path
            .read_dir()
            .unwrap_or_else(|err| panic!("unable read directory {path:?}: {err}"))
            .filter_map(Result::ok)
            .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
            .map(|f| f.path())
            .collect();
        let mut boards: Vec<(u64, String)> =
            paths.iter().map(get_info).filter_map(Result::ok).collect();
        boards.sort_by_key(|board| board.0);
        writeln!(db_file, "{header}")?;
        let mut last_hash = 0;
        for board in &boards {
            if board.0 != last_hash {
                writeln!(db_file, "{}", board.1)?;
            }
            last_hash = board.0;
        }
    } else if path.is_file() {
        todo!("adding individual files is not yet supported");
    }
    Ok(())
}

fn get_info<P: AsRef<Path>>(path: P) -> NesResult<(u64, String)> {
    let path = path.as_ref();
    let cart = Cart::from_path(path, RamState::default())?;
    let mut hasher = DefaultHasher::new();
    cart.prg_rom().hash(&mut hasher);
    let filename = path.file_name().unwrap_or_default();
    let hash = hasher.finish();
    let region = match filename.to_str() {
        Some(filename) => {
            if filename.contains("Europe") || filename.contains("PAL") {
                "PAL"
            } else {
                "NTSC"
            }
        }
        None => "NTSC",
    };
    let board = "";
    let pcb = "";
    let chip = "";

    let chr_rom_banks = cart.chr_rom().len() / (8 * 1024);
    let chr_ram_banks = cart.chr_ram().len() / (8 * 1024);
    let prg_rom_banks = cart.prg_ram().len() / (16 * 1024);
    let prg_ram_banks = cart.prg_ram().len() / (16 * 1024);
    let mirroring = cart.mirroring();

    Ok((
        hash,
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{:?},{},{:?}",
            hash,
            region,
            board,
            pcb,
            chip,
            cart.mapper_num(),
            prg_rom_banks,
            chr_rom_banks,
            chr_ram_banks,
            prg_ram_banks,
            cart.battery_backed(),
            mirroring,
            cart.submapper_num(),
            filename
        ),
    ))
}

#[derive(StructOpt, Debug)]
#[must_use]
struct Opt {
    #[structopt(
        help = "The NES ROM or a directory containing `.nes` ROM files. [default: current directory]"
    )]
    path: Option<PathBuf>,
}
