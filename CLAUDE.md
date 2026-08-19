# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

TetaNES is a cross-platform NES emulator. The workspace has three crates:

- **`tetanes-core`** — the emulation library (CPU/PPU/APU/mappers/cart). Published, aims for stronger
  API stability, and must compile on stable and MSRV `1.88` in addition to nightly.
- **`tetanes`** — the UI binary: `winit` event loop + `egui` GUI + `wgpu` renderer. Targets desktop
  and `wasm32-unknown-unknown` (web via `trunk`).
- **`tetanes-utils`** — unpublished dev binaries (`chrdump`, `generate_db`, `list_boards`,
  `screenshot`).

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

Both crates have tests, each with its own CI job. `tetanes-core`'s are the bulk of them, and most
are ROM snapshot tests.

```sh
cargo nextest run -p tetanes-core --all-features           # everything
cargo nextest run -p tetanes-core nestest                  # substring filter for one test
cargo nextest run -p tetanes-core common::tests::cpu::     # a whole ROM-test group
```

### Commit messages

Conventional Commits (`cliff.toml` / release-plz generate the changelog and releases from them).

A message is a **synopsis of the theme and the reason for it**, not a record of how the work went.
Leave out what the diff already says: implementation walkthroughs, verification narration ("191
tests pass"), plan phase numbers and scratch-doc references, enum-size and boxing bookkeeping,
benchmark tables, and references to sibling commits by hash. Rationale tied to a specific line
belongs in a `//` comment next to that line, where the next reader will actually find it. What
survives is the theme, the reason, and anything that would cost time to re-learn.

A `BREAKING CHANGE:` footer must be **one line**. `cliff.toml` renders
`commit.breaking_description`, and git-conventional truncates that at the first continuation line,
so a wrapped footer silently loses everything after the first line in the changelog.

## Architecture

Per-area detail lives in .claude/rules/ and loads when you open the matching files: mappers,
debugging support, the UI crate, and the ROM test harness.

### Emulation core

`ControlDeck` (`control_deck.rs`) is the public entry point: it owns a `Bus`, loads `Cart`s, and
exposes `clock_frame`, save states, rewind data, and `Action` handling.

```
ControlDeck → Bus → { Cpu, Ppu, Mapper, Memory, Apu, Input, WRAM }
```

`Bus` is the container the components are wired into, and the whole of the emulated state — a save
state, a rewind frame and a run-ahead snapshot are each exactly one `Bus`, which is why it holds
bus state and nothing else: the emulated components, plus what the bus itself needs to run them
(`ram_state`, the attached `debugger`, `disasm`). The session — video, run-ahead buffers,
`sram_dir`, config — stays on `ControlDeck`.

`Cpu` and `Ppu` are the state a 6502 and a 2C02 keep. **What they do is an `impl Bus` block**,
because an access moves the whole machine: reading a byte clocks the PPU, the APU and the board on
the way past, and a CHR fetch goes through the board's page tables. Those blocks live in the file
that owns the state they read — the CPU's in `cpu.rs`, the instruction set's in `cpu/instr.rs`, the
PPU's in `ppu.rs`, the ones that install a board or rebuild its page tables in `mapper.rs`,
`set_debugger` in `debug.rs` — not in `bus.rs`, which holds CPU-bus routing. What needs only a
component's own registers stays on that component (`Cpu::set_acc`, `Ppu::render_pixel`,
`Ppu::read_status`).

Naming, since one type now carries both address spaces:

| | reads | writes |
|---|---|---|
| CPU, spending a cycle (what `instr.rs` calls) | `Bus::read`, `peek` | `Bus::write` |
| CPU address decode alone | `Bus::cpu_bus_read`, `cpu_bus_peek` | `Bus::cpu_bus_write` |
| PPU address decode | `Bus::ppu_bus_read`, `ppu_bus_peek` | `Bus::ppu_bus_write` |
| cartridge, through the page tables | `Bus::chr_read`, `chr_peek` | `Bus::chr_write` |

`Bus::clock_instr` runs one instruction, `Bus::cpu_clock` is the per-CPU-cycle component clock, and
`Bus::ppu_clock`/`ppu_clock_to` drive the PPU.

`Mapper` and `Memory` hang off `Bus`, not off the `Ppu` — the PPU is the heaviest user but the CPU
reaches PRG through them too, so they belong to neither.

Components expose `clock`, `reset(ResetKind)`, `region`/`set_region`, `output` and
`save`/`load` as **inherent methods**, each forwarding to the components it owns. There are no
`Clock`/`Reset`/`Regional`/`Sample`/`Sram` traits: nothing in the workspace is generic over them —
the one bound that ever existed was `clock_to<T: Clock + TimerCycle + Sample>` in `apu.rs` — so they
buy no polymorphism and cost an import in every file plus a name clash whenever a type wants both
`Map` and `Clock`. `ResetKind` and `NesRegion` remain in `common.rs`. `memory.rs` provides `Memory`
— the page-table-addressed arena holding every cart region — plus `ConstArray` and `Buffer`.

**Adding a component method does not mean adding a trait.** Prefer an inherent method plus an
explicit forwarding call from the owner.

Save states, SRAM, and rewind all serialize component state with `serde` + `bincode` + deflate
(`fs.rs`, magic header + `SAVE_VERSION`). Changing a serialized field layout breaks existing save
states.

**A save state carries only mutable state.** `Memory` puts its immutable regions (PRG-ROM, CHR-ROM)
first and marks the boundary with `ram_start`; its hand-written `Serialize`/`Deserialize` store the
layout plus `data[ram_start..]` only, and the restore copies the ROM back in from the console
already running. `Bus::swap_state` is the single funnel for *every* restore path —
`Bus::load_state`, rewind, and run-ahead, which calls it directly — so it is also where a state
belonging to a different cart is rejected (`cpu::StateMismatch`) rather than left running one game's
RAM against another's ROM. `load_state` is that plus `keep_session_settings`, which run-ahead must
skip: a snapshot's settings are already the running console's, and the APU history that would come
back with them belongs to the timeline being discarded. Page tables are likewise absent, rebuilt by
`Bus::rebuild_mapper_state` from the restored mapper registers.

### Mappers

`Mapper` is an **enum with static dispatch**, not a boxed trait object, and that is deliberate for
performance. Each board implements the `Map` trait, where only `mirroring` is required and
everything else has a default, so a board writes exactly the hooks its hardware has: register
writes, `update_banks`, the `prg_read`/`chr_read` escape hatches for things no page entry can
describe, IRQ/DMA pending, and `clock`/`reset`/`region`/`output`. `Map` has no supertraits, and
`Mapper` carries the inherent `clock`/`reset`/`region`/`output` methods and forwards them down the
ownership tree.

Adding a board is four edits, and the stable serialization ids are the part that breaks save states
when got wrong. Both are in the mappers rule.

## Lint/style conventions worth knowing

- Lints live in `Cargo.toml`'s `[workspace.lints]` - treat those as the source
  of truth. Prefer `Mutex` over `RwLock` (std/parking_lot) without a perf
  justification. Avoid `unsafe` unless absolutely necessary.
- `typos` runs as part of `cargo make lint`. `typos.toml` contains the ignored
  words and patterns.
- `deny.toml` enforces license + security-advisory policy.
- Error handling: `Result<T, E>`/`Option<T>` for recoverable/absent values,
  `thiserror` for errors. Avoid `unwrap`/`expect` outside tests unless the
  invariant is guaranteed and documented.
- `tracing` for structured logging.
- Prefer `fs::copy` over `fs::rename` - the latter fails when `src`/`dst` are on
  different mount points.
- Favor the Actor pattern over shared-state locking. When locking is needed,
  `Arc<Mutex<_>>` over `RwLock` absent a measured perf need. Prefer turbofish
  (`collect::<HashMap<_, _>>()`) over manual type annotations. Encapsulate any
  `unsafe` in a safe abstraction and document it with a `# Safety` section.
- API design: use `Into`/`From`/`TryFrom` for conversions
  (`.into()`/`try_into()` at call sites avoids future churn), implement `Debug`
  for public types, use `#[non_exhaustive]` for enums expected to grow, and
  restrict visibility to the smallest scope needed (`pub(crate)` over `pub`).
- Before naming a new type, check for vocabulary collisions with existing names
  and prefer the established `module::Type` convention.

## Writing documentation

The full house style is the `writing-docs` skill, and `~/.claude/hooks/doc-lint.py` rejects the
mechanical half at write time. The rules that apply while writing:

- Say it once. State the fact, then stop.
- Describe the code as it stands, not its history. Git has the history.
- `///` and `//!` are for consumers. Rationale goes in a plain `//` beside the code.
- Name the mechanism, not the intent. Make the component the subject.
- One idea per sentence. No semicolon joining two clauses.
- No em-dash or en-dash. Use a comma, parens, or ` - `.
- US spelling: center, rigor. Contractions are fine, and so is "we".
- Wrap comments at 100 columns, markdown prose at 80.
- Every item gets a doc comment, and siblings get the same treatment.

## Additional guidelines

- Ask for confirmation before deleting any unchecked in files as this is
  permanent and unrecoverable data loss.
