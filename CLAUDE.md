# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

TetaNES is a cross-platform NES emulator. The workspace has three crates:

- **`tetanes-core`** — the emulation library (CPU/PPU/APU/mappers/cart). Published, aims for stronger
  API stability, and must compile on stable and MSRV `1.88` in addition to nightly.
- **`tetanes`** — the UI binary: `winit` event loop + `egui` GUI + `wgpu` renderer. Targets desktop
  and `wasm32-unknown-unknown` (web via `trunk`).
- **`tetanes-utils`** — unpublished dev binaries (`generate_db`, `list_boards`).

## Commands

The toolchain is pinned to **nightly** (`rust-toolchain.toml`) and edition 2024. `.cargo/config.toml`
sets nightly-only `RUSTFLAGS`; when invoking a non-nightly toolchain you must unset
`CARGO_ENCODED_RUSTFLAGS` (CI does this for the stable/1.88 clippy jobs).

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
nightly/stable/1.88.

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

`ControlDeck` (`control_deck.rs`) is the public entry point: it owns a `Bus`, loads `Cart`s, and
exposes `clock_frame`, save states, rewind data, and `Action` handling.

```
ControlDeck → Bus → { Cpu, Ppu, Mapper, Memory, Apu, Input, WRAM }
```

`Bus` is the container the components are wired into, and the whole of the emulated state — a save
state, a rewind frame and a run-ahead snapshot are each exactly one `Bus`, which is why it holds
emulated state and nothing else (the session — video, run-ahead buffers, `sram_dir`, config — stays
on `ControlDeck`).

`Cpu` and `Ppu` are the state a 6502 and a 2C02 keep. **What they do is an `impl Bus` block**,
because an access moves the whole machine: reading a byte clocks the PPU, the APU and the board on
the way past, and a CHR fetch goes through the board's page tables. Those blocks live in the file
that owns the state they read — the CPU's in `cpu.rs`, the instruction set's in `cpu/instr.rs`, the
PPU's in `ppu.rs` — not in `bus.rs`, which holds CPU-bus routing. What needs only a component's own
registers stays on that component (`Cpu::set_acc`, `Ppu::render_pixel`, `Ppu::read_status`).

Naming, since one type now carries both address spaces:

| | reads | writes |
|---|---|---|
| CPU, spending a cycle (what `instr.rs` calls) | `Bus::read`, `peek` | `Bus::write` |
| CPU address decode alone | `Bus::cpu_bus_read`, `cpu_bus_peek` | `Bus::cpu_bus_write` |
| PPU address decode | `Bus::ppu_bus_read`, `ppu_bus_peek` | `Bus::ppu_bus_write` |
| cartridge, through the page tables | `Bus::chr_read`, `chr_peek` | `Bus::chr_write` |

`Bus::clock_instr` runs one instruction, `Bus::cpu_clock` is the per-CPU-cycle component clock, and
`Bus::ppu_clock`/`ppu_clock_to` drive the PPU.

`Mapper` and `Memory` hang off `Bus`, not off the `Ppu` as they once did — the PPU is the heaviest
user but the CPU reaches PRG through them too, so they belong to neither.

Components expose `clock`, `reset(ResetKind)`, `region`/`set_region`, `output` and
`save`/`load` as **inherent methods**, each forwarding to the components it owns. These used to be
the `Clock`/`Reset`/`Regional`/`Sample`/`Sram` traits in `common.rs`; they were deleted because
nothing was ever generic over them — across the whole workspace there was exactly one bound,
`clock_to<T: Clock + TimerCycle + Sample>` in `apu.rs` — so they bought no polymorphism and cost an
import in every file plus a name clash whenever a type wanted both `Map` and `Clock`. `ResetKind`
and `NesRegion` remain in `common.rs`. `memory.rs` provides `Memory` — the page-table-addressed
arena holding every cart region — plus `ConstArray` and `Buffer`.

**Adding a component method does not mean adding a trait.** Prefer an inherent method plus an
explicit forwarding call from the owner.

**Doc comments are for consumers; rationale is for maintainers.** `///` and `//!` say what a thing
is and how to use it. What it used to be, why it changed, and the measurement that justified it go
in a plain `//` comment next to the code — `Bus::wram`'s "measured un-boxed: ~1.2% slower, keep it
boxed" is the pattern.

Save states, SRAM, and rewind all serialize component state with `serde` + `bincode` + deflate
(`fs.rs`, magic header + `SAVE_VERSION`). Changing a serialized field layout breaks existing save
states.

**A save state carries only mutable state.** `Memory` puts its immutable regions (PRG-ROM, CHR-ROM)
first and marks the boundary with `ram_start`; its hand-written `Serialize`/`Deserialize` store the
layout plus `data[ram_start..]` only, and `Bus::load_state` copies the ROM back in from the console
already running. `Bus::load_state` is the single funnel for *every* restore path — `load_state`,
rewind and run-ahead — so it is also where a state belonging to a different cart is rejected
(`cpu::StateMismatch`) rather than left running one game's RAM against another's ROM. Page tables are
likewise absent, rebuilt by `Bus::rebuild_mapper_state` from the restored mapper registers.

**A debugger callback is handed the whole `Bus`** (`debug.rs`), not one component's state, because
what a debugger needs differs per debugger — a CPU debugger wants registers and the disassembly
around PC, an APU viewer the channels, a hex viewer an arbitrary range, the PPU viewer CHR resolved
through the board. Each viewer's closure runs at the break point and copies out only what it ships
to its own thread; `ppu_viewer.rs`'s `PpuSnapshot` is one such choice, not the API. Core's part is
`Bus::copy_ppu_bus`, which fills a buffer with `$0000-$2FFF` as currently banked, so no consumer
needs board knowledge. The dot is the only trigger today; `Debugger` is the struct to extend when
breakpoints land.

### Mappers

`Mapper` is an **enum with static dispatch**, not a boxed trait object — this is deliberate for
performance. Each board implements the `Map` trait, where only `mirroring` is required and
everything else has a default, so a board writes exactly the hooks its hardware has: register
writes, `update_banks`, the `prg_read`/`chr_read` escape hatches for things no page entry can describe,
IRQ/DMA pending, and `clock`/`reset`/`region`/`output`. `Map` has no supertraits — `Mapper` is what
implements `Clock`/`Reset`/`Regional`/`Sample` and forwards them down the ownership tree.

**Adding a mapper is two edits:**

1. `tetanes-core/src/mapper/m0NN_<name>.rs` (files are named by primary mapper number; shared logic
   lives in un-numbered files like `mmc1.rs`, `mmc3.rs`, `vrc_irq.rs`).
2. One row in the `boards!` table in `mapper.rs`, which generates the `pub mod`, the `pub use`, the
   `Mapper` variant, the `From` impls, every dispatch arm, the mapper-number match in
   `Mapper::from_cart` (which `Cart::from_rom` calls), and the `print_layouts` entry.

A board module that publicly exports something *other* than the board type — so far only a revision
enum — needs a `pub use` next to the table. Optionally add a `test_roms!` group in `common.rs`.

Each row carries `= <id>`, its **stable serialization id: assign-once, never reused, never
renumbered.** That id is what goes on disk, so **rows can be reordered freely — keep the table in
mapper-number order.** The id *is* the board's primary (lowest) mapper number, so the table reads as
its own index; a board sharing a number with an earlier one (NINA-001 vs BNROM, both mapper 34)
takes `0x1000 + n` instead, above every real NES 2.0 number, and `Mapper::none()` is `0xFFFF` since
0 is NROM.

This is why `Serialize`/`Deserialize` for `Mapper` are hand-rolled: serde's derive tags variants by
*declaration position* and honours neither an explicit discriminant nor `#[repr]` (`enum E { A = 10 }`
still serializes as `0`), and bincode 2's own non-serde derive behaves the same, so the stability has
to live in our code to survive changing serializer. `mapper::tests::variant_tag_is_the_stable_id_not_the_declaration_position` pins the
bytes; `board_ids_are_unique_and_not_reserved` catches a duplicated id.

Where two boards share a mapper number (34 is BNROM or NINA-001 depending on CHR size) they carry
mutually exclusive `if` guards, so loader dispatch never depends on row order either.

A mapper number no row claims is `Error::Unimplemented`, so an unsupported ROM says so instead of
loading as open bus and showing a black screen. Tools that survey ROMs rather than run them use
`Cart::from_path_unmapped`/`from_rom_unmapped`, which skip board selection entirely.

Large boards are boxed in the enum (`Exrom`, `Namco163`, `Vrc6`, `BandaiFCG`, `SunsoftFme7`) to keep
`Mapper` small, currently 56 bytes — `print_layouts` prints every board's unboxed size so this stays
watchable. Boxing is a **measured** trade, not a size rule: it costs an indirection on boards clocked
every CPU cycle, and both directions have surprised us. See `benches/README.md`.

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
