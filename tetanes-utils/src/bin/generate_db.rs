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
    cart::{Cart, GameInfo},
    common::NesRegion,
    fs,
    mem::RamState,
    ppu::Mirroring,
};

const GAME_DB_TXT: &str = "tetanes-core/game_database.txt";
const GAME_DB: &str = "tetanes-core/game_db.dat";

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();
    let path = opt
        .path
        .unwrap_or_else(|| env::current_dir().unwrap_or_default());
    let header = "# CRC, Region, Mapper, PrgRomSize, ChrRomSize, ChrRamSize, PrgRamSize, Battery, Mirroring, SubMapper, Title";
    if path.is_dir() {
        let mut db_txt_file = BufWriter::new(
            File::create(GAME_DB_TXT).with_context(|| format!("failed to open {GAME_DB_TXT}"))?,
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
        writeln!(db_txt_file, "{header}")?;
        for game in &mut games {
            apply_corrections(game);

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
                db_txt_file,
                "  {crc32:8X}, {region}, {mapper}, {submapper}, {chr_banks}, {prg_rom_banks}, {prg_ram_banks}, {battery}, {mirroring:?}, {title:?}",
            )?;
            entries.push(GameInfo {
                crc32: *crc32,
                region: *region,
                mapper_num: *mapper,
                submapper_num: *submapper,
            });
        }
        fs::save(GAME_DB, &entries)?;
    } else if path.is_file() {
        todo!("adding individual games is not yet supported");
    }
    Ok(())
}

fn apply_corrections(game: &mut Game) {
    match game.crc32 {
        // Mapper 210 games incorrectly marked as Mapper 19
        0x808606F0 | 0x81B7F1A8 | 0xC247CC80 | 0xC47946D => {
            // Famista '91
            // Heisei Tensai Bakabon
            // Family Circuit '91
            // Chibi Maruko-chan: Uki Uki Shopping
            // Dream Master - TODO: Missing crc
            game.mapper = 210;
            game.submapper = 1;
        }
        0x1DC0F740 | 0x429103C9 | 0x46FD7843 | 0x47232739 | 0x6EC51DE5 | 0xADFFD64F
        | 0xD323B806 => {
            // Famista '92
            // Famista '93
            // Famista '94
            // Splatterhouse: Wanpaku Graffiti
            // Top Striker
            // Wagyan Land 2
            // Wagyan Land 3
            game.mapper = 210;
            game.submapper = 2;
        }
        _ => (),
    }
}

#[derive(Debug)]
#[must_use]
pub struct Game {
    crc32: u32,
    region: NesRegion,
    mapper: u16,
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
            mapper: cart.mapper_num(),
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
