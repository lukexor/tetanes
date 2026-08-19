---
paths:
  - "tetanes-core/src/mapper.rs"
  - "tetanes-core/src/mapper/**/*"
  - "docs/mapper/**/*"
---

# Mappers

How boards are declared, dispatched, and serialized. The `Mapper` enum and the `Map` trait
themselves are summarized in `CLAUDE.md`.

## Adding a mapper is four edits

1. `tetanes-core/src/mapper/m0NN_<name>.rs` (files are named by primary mapper number, and shared
   logic lives in un-numbered files like `mmc1.rs`, `mmc3.rs`, `vrc_irq.rs`).
2. One row in the `boards!` table in `mapper.rs`, which generates the `pub mod`, the `pub use`, the
   `Mapper` variant, the `From` impls, every dispatch arm, the mapper-number match in
   `Mapper::from_cart` (which `Cart::from_rom` calls), and the `print_layouts` entry.
3. An arm in `ControlDeck::update_mapper_revisions` (`control_deck.rs`), whose match over `Mapper`
   is exhaustive: a board with a user-selectable revision calls `set_revision`, everything else
   joins the no-op list. This one the compiler catches.
4. A row in the supported-mapper table in `README.md`. Nothing catches this one, and it is the step
   that gets skipped.

A board module that publicly exports something _other_ than the board type, so far only a revision
enum, needs a `pub use` next to the table. Optionally add a `test_roms!` group in `common.rs`.

## Stable serialization ids

Each row carries `= <id>`, its **stable serialization id: assign-once, never reused, never
renumbered.** That id goes on disk, so **rows can be reordered freely. Keep the table in
mapper-number order.** The id _is_ the board's primary (lowest) mapper number, so the table reads as
its own index. A board sharing a number with an earlier one (NINA-001 vs BNROM, both mapper 34)
takes `0x1000 + n` instead, above every real NES 2.0 number, and `Mapper::none()` is `0xFFFF` since
0 is NROM.

This is why `Serialize`/`Deserialize` for `Mapper` are hand-rolled. Serde's derive tags variants by
_declaration position_ and honours neither an explicit discriminant nor `#[repr]`
(`enum E { A = 10 }` still serializes as `0`), and bincode 2's own non-serde derive behaves the
same, so the stability has to live in our code to survive changing serializer.
`mapper::tests::variant_tag_is_the_stable_id_not_the_declaration_position` pins the bytes, and
`board_ids_are_unique_and_not_reserved` catches a duplicated id.

Where two boards share a mapper number (34 is BNROM or NINA-001 depending on CHR size) they carry
mutually exclusive `if` guards, so loader dispatch never depends on row order either.

A mapper number no row claims is `Error::Unimplemented`, so an unsupported ROM says so instead of
loading as open bus and showing a black screen. Tools that survey ROMs rather than run them use
`Cart::from_path_unmapped`/`from_rom_unmapped`, which skip board selection entirely.

## Boxing

Large boards are boxed in the enum (`Exrom`, `Namco163`, `Vrc6`, `BandaiFCG`, `SunsoftFme7`) to keep
`Mapper` small. `print_layouts` prints every board's unboxed size and the enum's own, so this stays
watchable without a number here to go stale. Boxing is a **measured** trade, not a size rule: it
adds an indirection on boards clocked every CPU cycle, and both directions have surprised us. See
`tetanes-core/benches/README.md`.

## Revisions and the game database

Boards that can't be identified from the header use `MapperRevision` (user/DB selectable, see
`MapperRevisionsConfig`), and `tetanes-core/game_db.dat` / `tetanes-core/game_database.txt` supply
per-ROM overrides by CRC. `tetanes-utils`' `generate_db` regenerates them, but it takes a directory
of `.nes` files that is not in the repo, so a correction to a handful of entries is easier made
against `game_database.txt` and re-encoded than by rebuilding from a corpus.

`docs/` is NES hardware reference, not project documentation. `docs/mapper/` contains Disch's mapper
documents, one `NNN.txt` per mapper number. Read those before reaching for nesdev.
