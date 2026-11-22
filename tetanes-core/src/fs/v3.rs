//! Reads files written before the MessagePack format: save states and replays at save version
//! `3`, battery-backed RAM at version `1`, and anything else bincode wrote.
//!
//! Everything that reads the old format calls into this module, so removing support is deleting
//! it and the branches that call it. A file read through here is written back in the current
//! format the next time it is saved.

use crate::{bus::Bus, fs};
use serde::de::DeserializeOwned;

/// The header version of a save state or replay written in this format.
pub const SAVE_VERSION: u8 = b'3';

/// The header version of battery-backed RAM written in this format.
pub const SRAM_VERSION: u8 = b'1';

/// The header version of a game database written in this format.
pub const GAME_DB_VERSION: u8 = b'1';

/// Decodes a bincode payload, returning the value and the number of bytes it took.
///
/// # Errors
///
/// If `data` does not hold a `T`.
pub fn decode<T>(data: &[u8]) -> fs::Result<(T, usize)>
where
    T: DeserializeOwned,
{
    bincode::serde::decode_from_slice(data, bincode::config::legacy())
        .map_err(|err| fs::Error::DeserializationFailed(err.to_string()))
}

/// Decodes a save state's console, returning it and the number of bytes it took.
///
/// # Errors
///
/// If `data` is not a version 3 console.
pub fn decode_state(data: &[u8]) -> fs::Result<(Bus, usize)> {
    // Every component decodes with its current type, which is only valid while that type still
    // reads what version 3 wrote. When one changes, its version 3 shape moves here with a
    // conversion, and the fixtures in `test_roms/save_states/v3/` fail until it does.
    decode(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        control_deck::{Config, ControlDeck},
        mapper::{Mapper, Nrom, Uxrom},
        memory::RamState,
        ppu::Mirroring,
    };
    use std::fs::File;

    fn fnv1a(data: &[u8]) -> u64 {
        data.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// Each fixture in `test_roms/save_states/v3/` was saved by the 0.18 release 300 frames into
    /// its ROM, which then drew the frame hashed here 60 frames later.
    macro_rules! v3_fixtures {
        ($($test:ident: $rom:expr, $fixture:expr, $hash:expr;)+) => {$(
            #[test]
            fn $test() {
                let mut deck = ControlDeck::with_config(
                    Config::default()
                        .with_ram_state(RamState::AllZeros)
                        .with_sram_dir(None),
                );
                let mut rom = File::open($rom).expect("rom");
                deck.load_rom($rom, &mut rom).expect("loads");
                let fixture = concat!("test_roms/save_states/v3/", $fixture, ".sav");
                deck.load_state(File::open(fixture).expect("fixture"))
                    .expect("a version 3 state loads");
                for _ in 0..60 {
                    let _ = deck.clock_frame().expect("clocks");
                }
                assert_eq!(fnv1a(deck.frame_buffer()), $hash, "resumes where 0.18 did");
            }
        )+};
    }

    v3_fixtures! {
        a_v3_nrom_state_resumes: "test_roms/spritecans.nes", "spritecans", 0x6775_ac81_8794_7cc6;
        a_v3_mmc1_state_resumes:
            "../tetanes/assets/roms/assimilate.nes", "assimilate", 0x5a30_976d_474e_6d8f;
        a_v3_uxrom_state_resumes:
            "../tetanes/assets/roms/cheril_the_goddess.nes", "cheril_the_goddess",
            0xe880_1f8d_1312_d07e;
        a_v3_cnrom_state_resumes:
            "test_roms/cpu/dummy_reads.nes", "dummy_reads", 0xac05_f790_a495_183c;
        a_v3_mmc3_state_resumes:
            "../tetanes/assets/roms/blade_buster.nes", "blade_buster", 0x8184_6059_e3fa_36f9;
        a_v3_mmc5_state_resumes:
            "test_roms/mapper/m005_exrom/exram.nes", "exram", 0xd4f9_f351_1b8b_f877;
        a_v3_bnrom_state_resumes:
            "../tetanes/assets/roms/data_man_demo.nes", "data_man_demo", 0x8e89_0b82_e785_e875;
    }

    /// Version 3 tagged a board with its `boards!` row's stable id, not the variant's declaration
    /// position. Moving a row must leave this passing, and renumbering an id must make it fail.
    #[test]
    fn a_board_is_tagged_by_its_stable_id() {
        let config = bincode::config::legacy();
        // The id is the board's primary mapper number: NROM 0, UxROM 2.
        let cases = [
            (Mapper::none(), 0xFFFF),
            (
                Mapper::from(Nrom {
                    mirroring: Mirroring::Vertical,
                }),
                0,
            ),
            (
                Mapper::from(Uxrom {
                    mirroring: Mirroring::Vertical,
                    prg_bank: 0,
                }),
                2,
            ),
        ];
        for (mapper, expected_id) in cases {
            let bytes = bincode::serde::encode_to_vec(&mapper, config).expect("serializes");
            let tag = u32::from_le_bytes(bytes[..4].try_into().expect("4-byte tag"));
            assert_eq!(
                tag, expected_id,
                "{mapper:?} must serialize with id {expected_id}"
            );

            let (restored, _) = decode::<Mapper>(&bytes).expect("round trips");
            assert_eq!(
                std::mem::discriminant(&restored),
                std::mem::discriminant(&mapper),
                "{mapper:?} must come back as the same board"
            );
        }
    }

    /// A version 3 state decodes its board with the board's current struct, so the bytes a board
    /// serializes to must not change while this module exists. `boards.txt` pins a hash of each.
    ///
    /// After moving a changed board's version 3 shape into this module, record the new hashes with
    /// `UPDATE_SNAPSHOT=1`. A board missing from the file was added after version 3, so no
    /// version 3 state of it exists.
    #[test]
    fn board_bytes_match_version_3() {
        use crate::mapper::{BOARD_IDS, test_utils};
        use std::{collections::BTreeMap, fmt::Write};

        const PINNED: &str = "test_roms/save_states/v3/boards.txt";

        let mut actual = BTreeMap::new();
        for &(board, id) in BOARD_IDS {
            // The same generic cart `reported_registers_are_named_and_appended` loads every board
            // from. A board that will not load from it is skipped there too.
            let Ok(num) = u16::try_from(id) else { continue };
            let mut cart = test_utils::page_indexed_cart(128 * 1024, 8 * 1024, 128 * 1024);
            cart.header.mapper_num = num;
            let Ok(mapper) = Mapper::from_cart(&mut cart) else {
                continue;
            };
            let bytes = bincode::serde::encode_to_vec(&mapper, bincode::config::legacy())
                .expect("serializes");
            actual.insert(board, fnv1a(&bytes));
        }

        if std::env::var("UPDATE_SNAPSHOT").is_ok() {
            let mut out = String::new();
            for (board, hash) in &actual {
                writeln!(out, "{board} {hash:#018x}").expect("formats");
            }
            std::fs::write(PINNED, out).expect("writes");
            return;
        }

        let pinned = std::fs::read_to_string(PINNED).expect("pinned hashes");
        let changed = pinned
            .lines()
            .filter_map(|line| line.split_once(' '))
            .filter(|&(board, hash)| {
                actual
                    .get(board)
                    .is_none_or(|actual| format!("{actual:#018x}") != hash)
            })
            .map(|(board, _)| board)
            .collect::<Vec<_>>();
        assert!(
            changed.is_empty(),
            "these boards no longer read their version 3 states: {changed:?}"
        );
    }

    /// Battery RAM from a release before the MessagePack format has to load, or the next unload
    /// overwrites the player's game with blank RAM.
    #[test]
    fn a_version_1_sram_loads() {
        let ram = vec![1u8, 2, 3];
        let payload =
            bincode::serde::encode_to_vec(&ram, bincode::config::legacy()).expect("serializes");
        let mut file = Vec::new();
        fs::write_header(&mut file, "1").expect("header");
        fs::encode(&mut file, &payload).expect("deflates");

        assert_eq!(
            fs::load_sram::<Vec<u8>>(file.as_slice()).expect("loads"),
            ram
        );
    }

    /// An id this build does not know must be a clean error, not a panic or the wrong board.
    #[test]
    fn an_unknown_board_id_fails_to_decode() {
        let config = bincode::config::legacy();
        let mapper = Mapper::from(Nrom {
            mirroring: Mirroring::Vertical,
        });
        let mut bytes = bincode::serde::encode_to_vec(&mapper, config).expect("serializes");
        bytes[..4].copy_from_slice(&9999u32.to_le_bytes());
        assert!(
            decode::<Mapper>(&bytes).is_err(),
            "an unknown board id must not decode"
        );
    }
}
