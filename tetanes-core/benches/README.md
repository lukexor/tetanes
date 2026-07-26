# Benchmarks

## Running

```sh
cargo make bench                      # spritecans.nes only (committed, always available)
cargo make bench -- path/to/rom.nes   # one specific ROM
TETANES_BENCH_ROMS=~/roms cargo make bench          # every .nes in a directory
TETANES_BENCH_ROMS="a.nes:b.nes" cargo make bench   # an explicit list
```

`cargo make bench` wraps the benchmark in `perf stat` pinned to core 0. Plain
`cargo bench --profile perf --bench clock_frame` works too and is quieter.

Profiling:

```sh
cargo make flamegraph -- path/to/rom.nes   # -> target/flamegraph.svg
cargo make perf-report -- path/to/rom.nes  # flat perf profile to stdout
```

## Methodology

Each iteration constructs a **fresh** `ControlDeck` and reloads the ROM rather than calling
`reset`. `Reset for Bus` clears WRAM but not mapper PRG-RAM/SRAM or mapper bank registers, so
resetting between iterations lets battery-backed saves and bank state carry over and each
iteration ends up measuring a different game state. Before this was fixed, Super Mario Bros.
reported a 21.9% coefficient of variation; with a fresh load per iteration it reports 0.17%.

`RamState::AllZeros` is used so RAM contents are deterministic.

Frames are clocked from power-on with **no input injected**, so commercial ROMs are measured on a
title or attract screen rather than in gameplay. That is stable and repeatable, which is what
regression testing needs, but it under-reports a busy gameplay frame. `spritecans.nes` is a sprite
stress ROM and serves as the pessimistic end of the range.

The benchmark calls `clock_frame()`, **not** `clock_frame_output()`, so `Video::apply_filter` and
the NTSC filter are **not** measured.

## Reading the output

A coefficient of variation (`cv`) under ~1% means the run is clean and a 2% change is real. If `cv`
climbs, suspect background load, CPU frequency scaling, or non-deterministic emulator state.

## Baseline

Recorded 2026-07-25. `--profile perf`, 10 iterations x 600 frames, 120 warmup.

`before` is the pre-optimization baseline; `now` is after the APU filter and mixer work.

| ROM | Mapper | before | now | delta |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 3.197 | 2.924 | -8.5% |
| Super Mario Bros. | 000 NROM | 3.237 | 2.960 | -8.6% |
| Legend of Zelda | 001 MMC1 | 3.187 | 2.880 | -9.6% |
| Super Mario Bros. 3 | 004 MMC3 | 3.360 | 3.040 | -9.5% |
| Punch-Out!! | 009 MMC2 | 3.074 | 2.756 | -10.3% |
| Castlevania III | 005 MMC5 | 4.277 | 3.863 | -9.7% |
| Akumajou Densetsu | 024 VRC6 | 3.737 | 3.428 | -8.3% |
| **geometric mean** | | **3.418** | **3.102** | **-9.2%** |

Nearly all of that came from `Fir::output`. Of the rest, only the MMC5 integer mixer path moved a
number it was aimed at (Castlevania III, -2.6%). The secondary-OAM and Game Genie changes measured
within noise and were kept for clarity rather than speed.

**Micro-optimization is now well past the point of diminishing returns.** Three of the four changes
after the FIR rewrite measured at or below noise. What remains is structural: `Ppu::clock` at ~32%
and `FilterChain::consume` at ~12% both need design changes, not tweaks.

### Page table vs `Banks` (`cargo bench --profile perf --bench page_table`)

Isolated read cost of the Phase 2 page table against the `Banks` + `Memory<Box<[u8]>>` +
`CIRam::mirror` path every mapper repeats today, over a PPU background-fetch address pattern:

| Formulation | ns/read |
|---|---|
| page table | **0.78** |
| banks + mirror | 1.25 |

The page table is **1.6x faster per read**. Put in frame-time terms, though, a frame does roughly
41,000 CHR fetches and 30,000 PRG reads, so 0.47 ns/read saved is only **~0.03 ms of a ~3 ms frame,
around 1%**. Real gains will be larger than that because the page table also removes the `Mapper`
enum dispatch wrapping each read - which this microbenchmark excludes - and because complex boards
do much more per read (`Exrom::chr_read` alone is 5.1% of Castlevania III).

**The conclusion stands that the mapper rework is justified by code reduction rather than speed.**
Expect low single-digit percentages on simple boards and more on MMC5.

### Machine noise

These numbers need a quiet machine. A run taken while the load average was ~6 reported Punch-Out at
21.8% cv and Castlevania III at 13.9% with a max of 6.5 ms - roughly 70% above its true figure. The
`cv` column is what catches this: **treat any ROM above ~2% cv as an invalid measurement and re-run**
rather than reading its mean.

### Current profile

`perf record` on Super Mario Bros. 3 (MMC3), after the changes above. Remaining targets, largest
first:

| Function | Share |
|---|---|
| `Ppu::clock` | 32.1% |
| `Ppu::bg_fetch_cycle` | 17.3% |
| `FilterChain::consume` | 11.7% |
| `ControlDeck::clock_instr` | 6.7% |
| `Apu::clock_sync` | 4.6% |
| `Ppu::oam_eval_cycle` | 4.2% |
| `Bus::read` | 4.1% |
| `Bus::cpu_clock` | 3.4% |

Roughly: PPU ~56%, APU ~24%, CPU ~10%, Bus ~7.5%. `fmaf`/`fmaf_with_fma`, previously 5.2%
combined, no longer appear at all, and `Fir::output` fell from 4.5% to 1.25%.

Mapper cost varies enormously by board, which is the whole reason the corpus exists. On
Castlevania III, **MMC5-specific code is 18.2% of frame time**: `Exrom::output` 9.0% (called every
CPU cycle for expansion audio), `Exrom::chr_read` 5.1%, `Exrom::clock` 4.1%. On Super Mario Bros.
no mapper symbol appears at all.

Note that `bg_fetch_cycle` is 13.9% even on NROM, where `Nrom::chr_peek` is two match arms - so it
is mostly genuine PPU fetch work, **not** mapper dispatch. Removing dispatch will not reclaim most
of it.

`FilterChain::consume` runs at CPU clock rate and walks six `SampledFilter` entries per cycle,
each ~64 bytes apart, so it touches five scattered cache lines per CPU cycle to do little more
than a float compare and add. Improving it likely needs a more compact hot representation of the
period counters, which is a layout change and therefore save-state-affecting - see the plan's
Phase 5.

Notes:

- **MMC5 costs ~1.04 ms/frame over NROM** (4.277 vs 3.237, +32%) and VRC6 ~0.50 ms (+15%). Mapper
  overhead is real on complex boards even though it is nearly invisible on NROM, which is why the
  corpus exists — the previous NROM-only benchmark could not see any of it.
- Full-LTO `--profile release` measured 3.007 vs `--profile perf` 3.014 on spritecans under the old
  benchmark: **LTO buys nothing here** because `tetanes-core` is a single crate. The `perf` profile
  (LTO off, debug symbols) is a faithful stand-in for release, so profiles are representative.
- `spritecans.nes` is also currently the sole PGO training workload, which biases branch layout
  toward mapper 0.
