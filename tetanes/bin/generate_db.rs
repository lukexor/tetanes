use anyhow::Context;
use clap::Parser;
use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};
use tetanes_core::{
    cart::{Cart, GameRegion},
    common::NesRegion,
    fs,
    mem::RamState,
    ppu::Mirroring,
};

const GAME_DB: &str = "game_database.txt";
const GAME_REGIONS: &str = "game_regions.dat";

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();
    let path = opt
        .path
        .unwrap_or_else(|| env::current_dir().unwrap_or_default());
    let header = "# CRC, Region, Mapper, PrgRomSize, ChrRomSize, ChrRamSize, PrgRamSize, Battery, Mirroring, SubMapper, Title";
    if path.is_dir() {
        let mut db_file = BufWriter::new(
            File::create(GAME_DB).with_context(|| format!("failed to open {GAME_DB}"))?,
        );
        let mut games = path
            .read_dir()
            .unwrap_or_else(|err| panic!("unable read directory {path:?}: {err}"))
            .filter_map(Result::ok)
            .filter(|f| f.path().extension() == Some(OsStr::new("nes")))
            .map(|f| f.path())
            .map(Game::new)
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        games.sort_by_key(|game| game.crc32);
        let mut entries = Vec::with_capacity(games.len());
        writeln!(db_file, "{header}")?;
        for game in &games {
            let Game {
                crc32,
                region,
                mapper,
                submapper,
                chr_banks,
                prg_rom_banks,
                prg_ram_banks,
                battery,
                mirroring,
                title,
            } = game;
            writeln!(
                db_file,
                "  {crc32:8X}, {region}, {mapper}/{submapper}, {chr_banks}, {prg_rom_banks}, {prg_ram_banks}, {battery}, {mirroring:?}, {title:?}",
            )?;
            entries.push(GameRegion {
                crc32: *crc32,
                region: *region,
            });
        }
        fs::save(GAME_REGIONS, &entries)?;
    } else if path.is_file() {
        todo!("adding individual files is not yet supported");
    }
    Ok(())
}

#[derive(Debug)]
#[must_use]
pub struct Game {
    crc32: u32,
    region: NesRegion,
    mapper: &'static str,
    submapper: u8,
    chr_banks: usize,
    prg_rom_banks: usize,
    prg_ram_banks: usize,
    battery: bool,
    mirroring: Mirroring,
    title: String,
}

impl Game {
    fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Game> {
        let path = path.as_ref();
        let cart = Cart::from_path(path, RamState::default())?;
        let mut crc32 = fs::compute_crc32(cart.prg_rom());
        if cart.has_chr_rom() {
            crc32 = fs::compute_combine_crc32(crc32, cart.chr_rom());
        }
        let filename = path.file_name().unwrap_or_default();
        let region = match filename.to_str() {
            Some(filename) => {
                if filename.contains("Europe") || filename.contains("PAL") {
                    NesRegion::Pal
                } else {
                    NesRegion::Ntsc
                }
            }
            None => NesRegion::Ntsc,
        };

        let chr_banks = cart.chr_rom().len() / (8 * 1024);
        let prg_rom_banks = cart.prg_ram().len() / (16 * 1024);
        let prg_ram_banks = cart.prg_ram().len() / (16 * 1024);
        let mirroring = cart.mirroring();

        Ok(Game {
            crc32,
            region,
            mapper: cart.mapper_board(),
            submapper: cart.submapper_num(),
            chr_banks,
            prg_rom_banks,
            prg_ram_banks,
            battery: cart.battery_backed(),
            mirroring,
            title: filename.to_string_lossy().to_string(),
        })
    }
}

#[derive(Parser, Debug)]
#[must_use]
struct Opt {
    /// The NES ROM or a directory containing `.nes` ROM files. [default: current directory]
    path: Option<PathBuf>,
}
