//! Cheats, in the forms a frontend hands them over in.
//!
//! libretro says only "here is a string" - each core decides what a code looks like, and a
//! frontend's cheat database carries whatever the cores for that system have historically taken.
//! For the NES that is Game Genie letters and a raw `address:value`, so both are accepted, and a
//! single entry may hold several of them joined by `+`.
//!
//! Every cheat is a [`Patch`]: a substitution applied to reads, which is what a Game Genie does in
//! hardware. A RAM cheat expressed that way holds its value against the game rather than flickering
//! against it, as a once-a-frame poke would.

use crate::log;
use tetanes_core::{control_deck::ControlDeck, genie::GenieCode, patch::Patch};

/// Separates the codes inside one cheat, as a frontend's database packs them.
const JOIN: char = '+';

/// What the frontend has told this core about, indexed as the frontend indexes it.
///
/// A disabled cheat keeps its slot and holds no patches, because libretro identifies a cheat by
/// position and RetroArch re-sends the whole list whenever one changes.
#[derive(Default)]
pub struct Cheats {
    by_index: Vec<Vec<Patch>>,
}

impl Cheats {
    /// Applies or removes the cheat at `index`, then rebuilds the console's table.
    ///
    /// A code that will not parse is reported and dropped: a frontend has no way to be told which
    /// entry it was, so the alternative is silently applying half of a multi-part cheat.
    pub fn set(&mut self, deck: &mut ControlDeck, index: usize, enabled: bool, code: &str) {
        let patches = if enabled { parse(code) } else { Vec::new() };
        if self.by_index.len() <= index {
            self.by_index.resize_with(index + 1, Vec::new);
        }
        self.by_index[index] = patches;
        self.rebuild(deck);
    }

    /// Drops every cheat.
    ///
    /// Also what a cart being ejected needs: ejecting one replaces the board but leaves the
    /// console's patch table alone, so a code entered for one game would otherwise still be
    /// substituting at that address in the next.
    pub fn reset(&mut self, deck: &mut ControlDeck) {
        self.by_index.clear();
        deck.clear_genie_codes();
    }

    /// Replaces the console's patches with what the frontend currently has enabled.
    ///
    /// Rebuilt wholesale rather than removing the patches of the cheat that changed: the console
    /// holds one patch per address, so two cheats covering one address share a slot and removing
    /// either by address would take the other's with it.
    fn rebuild(&self, deck: &mut ControlDeck) {
        deck.clear_genie_codes();
        for patch in self.by_index.iter().flatten() {
            deck.add_patch(*patch);
        }
    }
}

/// Reads one cheat, which is one or more codes joined by `+`.
///
/// All or nothing: a code that will not parse discards the whole entry, since a multi-part cheat
/// applied in part is a corrupted game rather than a cheat that half works.
fn parse(code: &str) -> Vec<Patch> {
    let mut patches = Vec::new();
    for part in code
        .split(|c: char| c == JOIN || c.is_whitespace())
        .filter(|part| !part.is_empty())
    {
        match parse_one(part) {
            Some(patch) => patches.push(patch),
            None => {
                log::error(&format!(
                    "ignoring the cheat \"{code}\": {part} is not a code"
                ));
                return Vec::new();
            }
        }
    }
    patches
}

/// Reads one code, in whichever of the two notations it is written.
fn parse_one(code: &str) -> Option<Patch> {
    // The separator is what tells them apart: a Game Genie code is letters only, so anything
    // carrying one is the raw form.
    match code.split_once([':', '-', '=']) {
        Some((addr, data)) => Some(Patch::new(
            u16::from_str_radix(addr.trim(), 16).ok()?,
            u8::from_str_radix(data.trim(), 16).ok()?,
            None,
        )),
        None => GenieCode::new(code.trim().to_ascii_uppercase())
            .ok()
            .map(|code| Patch::from(&code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetanes_core::{control_deck::Config, memory::RamState, video::VideoFilter};

    const ROM: &[u8] = include_bytes!("../../tetanes-core/test_roms/spritecans.nes");

    fn deck() -> ControlDeck {
        let mut deck = ControlDeck::with_config(Config {
            sram_dir: None,
            ram_state: RamState::AllZeros,
            filter: VideoFilter::Pixellate,
            ..Default::default()
        });
        deck.load_rom("test", &mut &ROM[..]).expect("loads");
        deck
    }

    fn applied(deck: &ControlDeck) -> Vec<Patch> {
        let mut patches: Vec<_> = deck.patches().copied().collect();
        patches.sort_unstable_by_key(|patch| patch.addr);
        patches
    }

    /// The raw form is the one a frontend's NES cheat database is mostly written in, because it
    /// reaches the work RAM a Game Genie's 15-bit address cannot.
    #[test]
    fn a_raw_code_becomes_a_patch() {
        assert_eq!(parse_one("00A2:10"), Some(Patch::new(0x00A2, 0x10, None)));
        assert_eq!(parse_one("00a2:10"), Some(Patch::new(0x00A2, 0x10, None)));
        assert_eq!(parse_one("6000-FF"), Some(Patch::new(0x6000, 0xFF, None)));
        assert_eq!(parse_one("07FF=01"), Some(Patch::new(0x07FF, 0x01, None)));
    }

    /// A Game Genie code carries its own address and compare byte, which is the whole point of the
    /// eight-letter form.
    #[test]
    fn a_genie_code_becomes_a_patch() {
        let six = parse_one("SXIOPO").expect("a six-letter code");
        assert!(six.addr >= 0x8000, "the genie only reaches ROM");
        assert_eq!(six.compare, None, "six letters carry no compare byte");

        let eight = parse_one("YEUZUGAA").expect("an eight-letter code");
        assert!(eight.compare.is_some(), "eight letters do");
        assert_eq!(
            parse_one("sxiopo"),
            Some(six),
            "case is the user's, not the code's"
        );
    }

    /// A frontend's database packs a multi-part cheat into one entry.
    #[test]
    fn one_entry_may_hold_several_codes() {
        assert_eq!(
            parse("00A2:10+00A3:20"),
            vec![
                Patch::new(0x00A2, 0x10, None),
                Patch::new(0x00A3, 0x20, None)
            ]
        );
        assert_eq!(
            parse("00A2:10 00A3:20"),
            vec![
                Patch::new(0x00A2, 0x10, None),
                Patch::new(0x00A3, 0x20, None)
            ],
            "whitespace separates too, since a user typing one will use it"
        );
    }

    /// Half a cheat is worse than none: the game gets an inconsistent set of substitutions and the
    /// player has no way to see which part landed.
    #[test]
    fn a_bad_code_discards_the_whole_entry() {
        assert!(parse("00A2:10+NOTACODE").is_empty());
        assert!(parse("ZZZZ").is_empty());
        assert!(parse("").is_empty());
        assert!(parse("00A2:XY").is_empty());
        assert!(parse("1FFFF:10").is_empty(), "past the address space");
    }

    /// The frontend addresses a cheat by position and re-sends the list when one changes, so a
    /// disabled entry has to keep its slot.
    #[test]
    fn disabling_one_cheat_leaves_the_others() {
        let mut deck = deck();
        let mut cheats = Cheats::default();

        cheats.set(&mut deck, 0, true, "00A2:10");
        cheats.set(&mut deck, 1, true, "00A3:20");
        assert_eq!(
            applied(&deck),
            vec![
                Patch::new(0x00A2, 0x10, None),
                Patch::new(0x00A3, 0x20, None)
            ]
        );

        cheats.set(&mut deck, 0, false, "00A2:10");
        assert_eq!(
            applied(&deck),
            vec![Patch::new(0x00A3, 0x20, None)],
            "the second cheat kept its index and its patch"
        );

        cheats.set(&mut deck, 0, true, "00A2:10");
        assert_eq!(applied(&deck).len(), 2, "and re-enabling brings it back");
    }

    /// The console keeps one patch per address, so removing a cheat by address alone would take a
    /// second cheat covering the same byte with it. Rebuilding is what avoids that.
    #[test]
    fn two_cheats_at_one_address_do_not_erase_each_other() {
        let mut deck = deck();
        let mut cheats = Cheats::default();

        cheats.set(&mut deck, 0, true, "00A2:10");
        cheats.set(&mut deck, 1, true, "00A2:20");
        assert_eq!(applied(&deck), vec![Patch::new(0x00A2, 0x20, None)]);

        cheats.set(&mut deck, 1, false, "00A2:20");
        assert_eq!(
            applied(&deck),
            vec![Patch::new(0x00A2, 0x10, None)],
            "the first cheat is still applied"
        );
    }

    /// Ejecting a cart replaces the board but leaves the console's patch table alone, so the reset
    /// has to be ours - otherwise one game's cheat keeps substituting in the next.
    #[test]
    fn a_reset_clears_the_console_not_just_the_table() {
        let mut deck = deck();
        let mut cheats = Cheats::default();

        cheats.set(&mut deck, 0, true, "00A2:10");
        cheats.reset(&mut deck);
        assert!(applied(&deck).is_empty());

        cheats.set(&mut deck, 0, true, "00A3:20");
        assert_eq!(
            applied(&deck),
            vec![Patch::new(0x00A3, 0x20, None)],
            "and the indices start again from empty"
        );
    }

    /// A cheat has to actually change what the game reads, which is the only assertion here that
    /// tests the console rather than the bookkeeping.
    #[test]
    fn an_applied_cheat_changes_what_the_console_reads() {
        let mut deck = deck();
        let mut cheats = Cheats::default();
        deck.wram_mut()[0xA2] = 0x01;

        cheats.set(&mut deck, 0, true, "00A2:10");
        assert_eq!(deck.bus().peek(0x00A2), 0x10, "the cheat substitutes");

        cheats.reset(&mut deck);
        assert_eq!(deck.bus().peek(0x00A2), 0x01, "and clearing puts it back");
    }
}
