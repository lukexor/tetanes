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

### Page table vs `Banks` (historical)

Measured with a `page_table` benchmark that has since been deleted along with `Banks` itself.
Isolated read cost of the Phase 2 page table against the `Banks` + `Memory<Box<[u8]>>` +
`CIRam::mirror` path every mapper repeated at the time, over a PPU background-fetch address pattern:

| Formulation | ns/read |
|---|---|
| page table | **0.78** |
| banks + mirror | 1.25 |

The page table is **1.6x faster per read**. Put in frame-time terms, though, a frame does roughly
41,000 CHR fetches and 30,000 PRG reads, so 0.47 ns/read saved is only **~0.03 ms of a ~3 ms frame,
around 1%**. Real gains will be larger than that because the page table also removes the `Mapper`
enum dispatch wrapping each read - which this microbenchmark excludes - and because complex boards
do much more per read (MMC5's CHR read alone was 5.1% of Castlevania III).

**The conclusion stands that the mapper rework is justified by code reduction rather than speed.**
Expect low single-digit percentages on simple boards and more on MMC5. (Measured afterwards: right
about the simple boards, wrong about MMC5 — see below.)

### After the mapper rework

Recorded 2026-07-26, once every board was serving reads from page tables. Both columns were
measured back to back in the same session on the same machine, because the 2026-07-25 table above
was taken on a quieter one and differs from a re-measurement of the same commit by up to 1.5% -
enough to swamp what is being measured here.

`before` is `e77009b`, the last commit before the page tables landed. `ported` is with every board
on page tables but the old path still compiled in beside it; `after` is once that path was deleted,
which removes a branch from every read, write and CHR fetch.

| ROM | Mapper | before | ported | after | delta |
|---|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 2.959 | 2.845 | 2.791 | -5.7% |
| Super Mario Bros. | 000 NROM | 2.977 | 2.858 | 2.854 | -4.1% |
| Legend of Zelda | 001 MMC1 | 2.917 | 2.795 | 2.770 | -5.0% |
| Super Mario Bros. 3 | 004 MMC3 | 3.068 | 2.990 | 2.984 | -2.7% |
| Punch-Out!! | 009 MMC2 | 2.791 | 2.785 | 2.733 | -2.1% |
| Castlevania III | 005 MMC5 | 3.879 | 3.910 | 3.886 | **+0.2%** |
| Akumajou Densetsu | 024 VRC6 | 3.441 | 3.313 | 3.295 | -4.2% |
| **geometric mean** | | **3.129** | **3.049** | **3.022** | **-3.4%** |

Deleting the transitional path is worth about 0.9% on its own - individually most of those ROMs
move within noise, but every one of them moves the same way.

This lands close to the microbenchmark's prediction for simple boards and **contradicts it for
MMC5**, which the plan expected to gain the most and which instead came out level. Its reads
were already cheap; what the port replaced was `Exrom::chr_peek`'s match with a page lookup plus a
`Map::chr_read` call that MMC5 - alone among the boards - takes on every fetch, because extended
attributes, fill mode and the vertical split are all synthesised rather than fetched. The board also
now re-derives its CHR bank set per fetch instead of latching it, which is what made the sprite-size
rule correct. Both were paid for with accuracy, not lost to overhead.

MMC2 is the cautionary tale. Its first ported form called `sync` from `ppu_bus_addr`, which the CHR
latch triggers thousands of times a frame, so every latch flip rebuilt 32 PRG pages, 8 CHR pages and
the nametables: Punch-Out!! went from 2.791 to **3.336 ms/frame, a 20% regression** that the corpus
caught and an NROM-only benchmark never would have. Re-mapping only the 4K window whose latch moved
restored it. **A board's `sync` is a cold-path routine; anything reachable from a per-fetch hook has
to touch only what changed.**

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
| `Ppu::clock` | 33.5% |
| `FilterChain::consume` | 13.6% |
| `Ppu::bg_fetch_cycle` | 13.4% |
| `ControlDeck::clock_instr` | 7.1% |
| `Apu::clock_sync` | 6.3% |
| `Ppu::oam_eval_cycle` | 4.1% |
| `Bus::cpu_clock` | 3.5% |
| `Ppu::fetch_bg_nt_byte` | 2.4% |
| `Bus::read` | 2.3% |

Roughly: PPU ~54%, APU ~26%, CPU ~10%, Bus ~6%. `fmaf`/`fmaf_with_fma`, previously 5.2% combined,
no longer appear at all, and `Fir::output` fell from 4.5% to ~1%. **No MMC3 symbol appears at all**
now that `Txrom` holds only registers - the board's entire contribution is the `Mmc3::clock_irq`
call inlined into the PPU fetch path.

Mapper cost varies enormously by board, which is the whole reason the corpus exists. On
Castlevania III, **MMC5-specific code is 13.2% of frame time**, down from 18.2% before the port:
`Exrom::chr_read_hook` 4.6%, `Exrom::output` 4.5% (called every CPU cycle for expansion audio),
`Exrom::clock` 4.1%. The hook is the one that did not shrink, because it now runs on every PPU
fetch rather than only on the reads MMC5 had to synthesise. On Super Mario Bros. no mapper symbol
appears at all.

Note that `bg_fetch_cycle` is 13.9% even on NROM, where `Nrom::chr_peek` is two match arms - so it
is mostly genuine PPU fetch work, **not** mapper dispatch. Removing dispatch will not reclaim most
of it.

`FilterChain::consume` runs at CPU clock rate and walks six `SampledFilter` entries per cycle,
each ~64 bytes apart, so it touches five scattered cache lines per CPU cycle to do little more
than a float compare and add. Improving it likely needs a more compact hot representation of the
period counters, which is a layout change and therefore save-state-affecting - see the plan's
Phase 5.

Notes:

- **MMC5 costs ~1.03 ms/frame over NROM** (3.886 vs 2.854, +36%) and VRC6 ~0.44 ms (+15%). Mapper
  overhead is real on complex boards even though it is nearly invisible on NROM, which is why the
  corpus exists — the previous NROM-only benchmark could not see any of it, and would also have
  missed the 20% MMC2 regression above.
- Full-LTO `--profile release` measured 3.007 vs `--profile perf` 3.014 on spritecans under the old
  benchmark: **LTO buys nothing here** because `tetanes-core` is a single crate. The `perf` profile
  (LTO off, debug symbols) is a faithful stand-in for release, so profiles are representative.
- `spritecans.nes` is also currently the sole PGO training workload, which biases branch layout
  toward mapper 0.
