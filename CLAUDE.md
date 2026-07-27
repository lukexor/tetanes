# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

TetaNES is a cross-platform NES emulator. The workspace has three crates:

- **`tetanes-core`** — the emulation library (CPU/PPU/APU/mappers/cart). Published, aims for stronger
  API stability, and must compile on stable and MSRV `1.85` in addition to nightly.
- **`tetanes`** — the UI binary: `winit` event loop + `egui` GUI + `wgpu` renderer. Targets desktop
  and `wasm32-unknown-unknown` (web via `trunk`).
- **`tetanes-utils`** — unpublished dev binaries (`generate_db`, `list_boards`).

## Commands

The toolchain is pinned to **nightly** (`rust-toolchain.toml`) and edition 2024. `.cargo/config.toml`
sets nightly-only `RUSTFLAGS`; when invoking a non-nightly toolchain you must unset
`CARGO_ENCODED_RUSTFLAGS` (CI does this for the stable/1.85 clippy jobs).

Most workflows go through [`cargo-make`](https://github.com/sagiegurari/cargo-make) (`Makefile.toml`):

```sh
cargo make dev -- path/to/rom.nes     # debug run (opt-level 1, playable)
cargo make run -- path/to/rom.nes     # release run
cargo make dev-web / run-web          # trunk serve, dev/release
cargo make lint                       # clippy for native + wasm, --all-features
cargo make check-fmt
cargo make test -- <args>             # cargo nextest run --all-features --no-fail-fast
cargo make docs                       # rustdoc native + wasm (CI treats warnings as errors)
cargo make bench                      # perf stat around the clock_frame benchmark
cargo make build                      # PGO build (cargo-pgo)
```

Clippy must be clean with `-D warnings` for: native `tetanes`, wasm `tetanes`, and `tetanes-core` on
nightly/stable/1.85.

### Tests

Only `tetanes-core` has tests; the `tetanes` crate has none (its CI job is commented out).

```sh
cargo nextest run -p tetanes-core --all-features           # everything
cargo nextest run -p tetanes-core nestest                  # substring filter for one test
cargo nextest run -p tetanes-core common::tests::cpu::     # a whole ROM-test group
UPDATE_SNAPSHOT=1 cargo nextest run -p tetanes-core <test> # rewrite expected frame hashes
```

Most tests are **ROM snapshot tests**. The harness lives in `tetanes-core/src/common.rs` (`mod tests`):
the `test_roms!` macro declares one `#[test]` per named ROM, expectations come from
`test_roms/<dir>/tests.json` (frame number → frame-buffer or audio hash, plus optional `Action`s to
inject), and rendered PNGs land in `tetanes-core/test_results/{pass,fail}/`. Adding a ROM test means
adding the ROM + a `tests.json` entry + a name in the relevant `test_roms!` invocation at the bottom
of `common.rs`. `UPDATE_SNAPSHOT=1` rewrites `tests.json` in place — only use it when a hash change is
intentional and the resulting PNG has been eyeballed.

Commit messages follow Conventional Commits (`cliff.toml` / release-plz generate the changelog and
releases from them).

## Architecture

### Emulation core

`ControlDeck` (`control_deck.rs`) is the public entry point: it owns a `Cpu`, loads `Cart`s, and
exposes `clock_frame`, save states, rewind data, and `Action` handling. Ownership is a strict tree:

```
ControlDeck → Cpu → Bus → { Ppu → Mapper, Apu, Input, WRAM }
```

The `Mapper` lives inside the `Ppu` (CHR/CIRAM access is the hot path); the CPU reaches PRG through
the bus. Cross-cutting behavior is expressed as small traits in `common.rs` — `Clock`, `Reset`,
`Regional`, `Sram`, `Sample` — implemented by nearly every component and forwarded down the tree.
`mem.rs` provides `Memory<D>`, `ConstArray`, and `Banks` (windowed bank translation used by every
mapper).

Save states, SRAM, and rewind all serialize component state with `serde` + `bincode` + deflate
(`fs.rs`, magic header + `SAVE_VERSION`). Changing a serialized field layout breaks existing save
states.

### Mappers

`Mapper` is an **enum with static dispatch**, not a boxed trait object — this is deliberate for
performance. Each board implements the `Map` trait, where only `mirroring` is required and
everything else has a default, so a board writes exactly the hooks its hardware has: register
writes, `sync`, the `prg_read`/`chr_read` escape hatches for things no page entry can describe,
IRQ/DMA pending, and `clock`/`reset`/`region`/`output`. `Map` has no supertraits — `Mapper` is what
implements `Clock`/`Reset`/`Regional`/`Sample` and forwards them down the ownership tree.

**Adding a mapper is two edits:**

1. `tetanes-core/src/mapper/m0NN_<name>.rs` (files are named by primary mapper number; shared logic
   lives in un-numbered files like `mmc1.rs`, `mmc3.rs`, `vrc_irq.rs`).
2. One row in the `boards!` table in `mapper.rs`, which generates the `pub mod`, the `pub use`, the
   `Mapper` variant, the `From` impls, every dispatch arm, the mapper-number match in
   `Mapper::from_cart` (which `Cart::new` calls), and the `print_layouts` entry.

A board module that publicly exports something *other* than the board type — so far only a revision
enum — needs a `pub use` next to the table. Optionally add a `test_roms!` group in `common.rs`.

**Row order in `boards!` is the enum's variant order, and `bincode` serializes enum variants by
index — reordering rows invalidates every existing save state. Add new boards at the end.** Where
two boards share a mapper number (34 is BNROM or NINA-001 depending on CHR size) they carry
mutually exclusive `if` guards, so loader dispatch never depends on row order.

Large boards are boxed in the enum (`Exrom`, `Namco163`, `Vrc6`, `BandaiFCG`, …) to keep `Mapper`
small — the `print_layouts` test exists to watch struct/enum sizes for cache behavior.

Boards that can't be identified from the header use `MapperRevision` (user/DB selectable, see
`MapperRevisionsConfig`), and `game_db.dat` / `game_database.txt` (regenerate with
`tetanes-utils`' `generate_db`) supplies per-ROM overrides by CRC.

### UI crate

`Nes` (`nes.rs`) is the winit `ApplicationHandler`, holding a `State` machine
(`Suspended → Pending → Running → Exiting`) because wgpu/window resources are created asynchronously.
`Running` owns:

- **`Emulation`** (`nes/emulation.rs`) — runs the `ControlDeck`. It has two backends: `Threads::Multi`
  (emulation on its own thread, self-clocking, woken via `unpark`) and `Threads::Single` (clocked
  from the event loop). Selection is `cfg.emulation.threaded` (CLI `--no-threaded`) AND
  `available_parallelism() > 1`, so single-threaded is what runs on wasm. Frames reach the renderer over a `thingbuf`
  channel with a `FrameRecycle` pool to avoid per-frame allocation.
- **`Renderer`** (`nes/renderer.rs`) — egui + wgpu, multi-viewport aware, with the emulator frame
  drawn through a custom `painter`/`shader`/`texture` path and the GUI in `renderer/gui.rs`.

All communication is via `NesEvent` (`nes/event.rs`) pushed through a winit `EventLoopProxy`
(`NesEventProxy`), split into `EmulationEvent`, `RendererEvent`, `ConfigEvent`, `DebugEvent`, and
`UiEvent`. Adding a feature that crosses the emulation/UI boundary means adding a variant there
rather than sharing state.

### Platform abstraction

Both crates use the same pattern: a public façade module that `pub use`s a `sys::` implementation
selected by `cfg`, with parallel `os.rs` / `wasm.rs` files (`tetanes/src/sys/{platform,logging,thread,info}/`,
`tetanes-core/src/sys/{fs,time}/`). Capability checks at runtime go through
`platform::Feature` (`Filesystem`, `Storage`, `Suspend`, `ScreenReader`, …) rather than raw `cfg`
in UI code. Anything touching files, threads, time, or clipboard needs both sides implemented — the
wasm clippy/doc CI jobs will catch omissions.
