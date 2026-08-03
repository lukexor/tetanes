# Benchmarks

## Running

```sh
cargo make bench                      # spritecans.nes only (committed, always available)
cargo make bench -- path/to/rom.nes   # one specific ROM
TETANES_BENCH_ROMS=path/to/roms cargo make bench    # every .nes in a directory
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
`reset`. `Bus::reset` clears WRAM but not mapper PRG-RAM/SRAM or mapper bank registers, so
resetting between iterations lets battery-backed saves and bank state carry over and each
iteration ends up measuring a different game state: Super Mario Bros. reports a 21.9% coefficient
of variation that way, against 0.17% with a fresh load per iteration.

`RamState::AllZeros` is used so RAM contents are deterministic.

Frames are clocked from power-on with **no input injected**, so commercial ROMs are measured on a
title or attract screen rather than in gameplay. That is stable and repeatable, which is what
regression testing needs, but it under-reports a busy gameplay frame. `spritecans.nes` is a sprite
stress ROM and serves as the pessimistic end of the range.

The benchmark calls `clock_frame()` **and** `frame_buffer()` each frame, so `Video::apply_filter`
and the NTSC filter are measured by default - every real frame is filtered, and leaving it out
under-reports frame time by 2.9% (measured 2026-08-01, interleaved A/B on the 7-ROM corpus:
2.262 against 2.327 geomean). Set `TETANES_BENCH_NO_OUTPUT=1` to time the CPU/PPU/APU core alone,
which is what you want when A/B-ing a core change: the filter is a constant offset that dilutes
the delta.

> **Every baseline recorded below predates that default flipping (2026-07) and excluded the
> filter.** Compare them against `TETANES_BENCH_NO_OUTPUT=1` runs, not against the current default.

## Comparing two builds

**A low `cv` within a run does not make two runs comparable.** Measured 2026-07-28 while A/B-ing the
ownership flattening, on the 7-ROM corpus with `TETANES_BENCH_NO_OUTPUT=1`:

| round | geometric mean |
|---|---|
| A run 1 | 2.949 |
| B run 1 | 2.875 |
| A run 2 | **2.860** |
| B run 2 | 2.871 |

A run 1 and A run 2 are **the same binary**, 3.1% apart, with every ROM reporting `cv` under 1.1%.
The first run of a session is systematically slow - caches, CPU frequency ramp, whatever the machine
was doing beforehand - and over a long session the whole machine drifts slower, so runs an hour
apart are not comparable at all. Taking `before` and `after` as one run each produced a confident,
entirely fictional 2.5% regression, and a plausible cache-line story to explain it.

So, to compare two builds:

1. **Build both first**, so no compile overlaps a measurement.
2. **Interleave**: A, B, A, B. Never all of A then all of B.
3. **Discard the first round.**
4. Run nothing else - no test suite, no `cargo check` - while measuring. Compilation on other cores
   perturbs a `taskset`-pinned run through turbo budget and memory bandwidth.
5. Treat any ROM above ~2% `cv` in a round as an invalid data point for that round.

Anything under ~1% between two interleaved builds is below this machine's noise floor; do not
report it as a win or a loss.

## Reading the output

A coefficient of variation (`cv`) under ~1% means the run is clean and a 2% change is real. If `cv`
climbs, suspect background load, CPU frequency scaling, or non-deterministic emulator state.

## Against MesenCE

Recorded 2026-08-01, `--profile release`, unpinned on a quiet 16-core machine, every ROM under
0.6% cv. `TETANES_BENCH_NO_OUTPUT=1` against MesenCE's default (core-only) mode, which is the
matched pair - MesenCE runs its video filter on the `VideoDecoder` thread, so its default number
excludes filtering just as `NO_OUTPUT` does here. MesenCE figures are its own benchmark on the same
machine and corpus.

| ROM | Mapper | TetaNES | MesenCE | gap |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 1.708 | 1.625 | +5.1% |
| Super Mario Bros. | 000 NROM | 1.739 | 1.631 | +6.6% |
| Legend of Zelda | 001 MMC1 | 1.732 | 1.600 | +8.3% |
| Super Mario Bros. 3 | 004 MMC3 | 1.937 | 1.687 | +14.8% |
| Punch-Out!! | 009 MMC2 | 1.669 | 1.582 | +5.5% |
| Castlevania III | 005 MMC5 | 2.811 | 2.534 | +10.9% |
| Akumajou Densetsu | 024 VRC6 | 2.314 | 2.241 | +3.3% |
| **geometric mean** | | **1.953** | **1.812** | **+7.8%** |

**Compare like with like or this number is wrong by half.** Timing the TetaNES default against
MesenCE's default reads as a ~25% gap, because it puts `Video::apply_filter` on one side and
nothing on the other. Both binaries here are non-PGO and generic x86-64 (MesenCE passes no
`-march` at all), so the comparison is architectural.

## On target: Raspberry Pi 5 (2026-08-03)

The original goal of the perf campaign was 60fps on a Raspberry Pi. Measured on a Pi 5 Model B
(Cortex-A76 x4 @ 2.4 GHz, Rocky 10), `--profile perf` built for `aarch64-unknown-linux-musl` with
`rust-lld` (static, no cross-glibc concerns), performance governor, `taskset`-pinned. Same 7-ROM
corpus.

| mode | geomean ms/frame | worst (Castlevania III) |
|---|---|---|
| core only (`NO_OUTPUT=1`) | 3.275 | 5.20 |
| core + Pixellate (`TETANES_BENCH_FILTER=pixellate`) | 3.371 | 5.23 |
| core + NTSC (default) | 3.451 | 5.31 |

Every mode clears the 16.6 ms/60fps budget by at least 3x, NTSC filter included - the filter costs
~5% here, same share as on x86. Extrapolating A76@2.4 to a Pi 4's A72@1.5 (~2.5x single-thread)
puts the worst case around 13 ms: under budget, but MMC5 titles leave limited headroom for the UI
stack on that board. Measured, not extrapolated: the Pi 5 meets the goal in every mode.

Two architectural differences from the x86 findings, from `perf` on the worst-case ROM:

- **The dot loop is not cache-bound here either, but it *is* branch-bound.** IPC 2.26, L1d misses
  0.03%, L1i 0.14% - the A76's 64 KB L1s hold the whole working set - but the branch-miss rate is
  2.06%, roughly 15% of all cycles at this machine's mispredict penalty. The same branches a
  desktop predictor eats for free are a real tax on this core.
- **The misses are not a hotspot.** Attribution mirrors the cycle profile (`ppu_clock` 36%,
  `bg_fetch_cycle` 13%, MMC5 hooks ~18%): it is the emulator's inherent data-dependent branching,
  not one convertible select. Chasing it means branchless dot-loop reformulations - the same shape
  of change that measured null on x86 - and with 3x headroom there is no need. A future
  ARM-focused session should A/B on target; run-to-run variance there is 0.02-0.34% cv, far
  tighter than x86.

The measurement setup survives on the Pi in `~/bench` (binary, ROMs, `perf`); the governor reverts
to `ondemand` on reboot.

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

### APU mixing: denormals and the per-cycle filter chain

Recorded 2026-07-29. `--profile perf`, 5 iterations x 600 frames, `TETANES_BENCH_NO_OUTPUT=1`,
interleaved A/B/C over three rounds. Two independent changes, measured separately because the
second is only visible once the first is in:

| build | geomean | delta |
|---|---|---|
| before | 2.762 | |
| + denormal flush | 2.598 | **-5.9%** |
| + mix only on the cycles the chain samples | 2.413 | **-12.6%** total |

Per ROM, before -> after: spritecans 2.464 -> 2.176, Super Mario Bros. 2.594 -> 2.091, Zelda
2.557 -> 2.242, SMB3 2.753 -> 2.304, Punch-Out!! 2.401 -> 2.130, Castlevania III 3.711 -> 3.410,
Akumajou Densetsu 3.059 -> 2.787. With the NTSC filter on (the app's real path) 2.806 -> 2.488.

**Denormals were the single largest line item in the APU, and were invisible in a time profile.**
The 90 Hz high-pass decays ~0.1% per sample, so every time a game goes quiet the chain spends
thousands of samples in the denormal range, and the FIR then multiplies all 161 taps of a ring full
of them on every output sample. `perf stat -e fp_assist.any` reported **7.9M assists** in a
1440-frame run at ~100 cycles each. Flushing anything under `1e-20` to zero took it to **0**.

The mixing restructure is the more interesting measurement, because **instruction count and cycle
count moved in opposite directions.** Mixing only on the ~1,470 cycles a frame where the filter
chain actually samples, rather than all ~29,800, cut mixing from 20.6G to 3.9G instructions - a
5.3x reduction - and *raised* cycles from 9.6G to 12.8G. IPC fell from 2.15 to 0.30. The per-cycle
loop had been acting as latency-hiding filler for the FIR, and removing it exposed 49M denormal
assists that the surrounding work had been absorbing. So the two changes are not additive by
accident: **the restructure is a 20% regression without the denormal fix and a 6.5% win with it.**

The lesson for the next round of this: on this workload `perf stat` counters are worth more than a
flat profile. A profile said `FilterChain::consume` was 7.7% and `Fir::output` 1.3%; the truth was
that the FIR was ~10x its apparent cost and the cost was microarchitectural.

### APU: skip-ahead timers

Recorded 2026-07-29, same protocol, interleaved over three rounds against the commit above.

| build | geomean | delta |
|---|---|---|
| after the mixing work | 2.432 | |
| + skip-ahead timers | 2.261 | **-7.0%** |

Per ROM: spritecans 2.029, Super Mario Bros. 1.990, Zelda 2.074, SMB3 2.217, Punch-Out!! 1.934,
Castlevania III 3.230, Akumajou Densetsu 2.613. With the NTSC filter on, 2.338.

`Timer::run_to` collapses the cycles between one waveform step and the next into a single
subtraction, so a pulse at period 200 costs ~25 iterations per 10,000-cycle block rather than
10,000. That in turn removes the reason the 240 KiB per-cycle `channel_outputs` array existed: the
mixer reads each channel's level directly at the cycle it wants, which is only the ~1,470 cycles a
frame the filter chain samples.

Cumulatively over the three APU changes, **2.762 -> 2.261, -18.1%**, and the APU's share of core
frame time on SMB3 went ~23% -> ~9.5%. What is left is `clock_channels` 3.2%, `clock_lazy` 2.9%,
`clock_to` 2.5%. The PPU is now 68%.

Two things nearly went wrong, both worth knowing before touching this again:

- **A channel can legitimately sit ahead of the cycle being asked for.** `Dmc::reset` parks its
  timer one cycle forward on purpose (there is a FIXME saying so, and the DMA tests depend on it)
  and `Triangle::reset` does not reset its timer at all. The `while cycle() < target` loop being
  replaced quietly did nothing in that case; `run_to`'s subtraction wrapped and fired a spurious
  expiry instead. In a debug build it panicked; in the optimized build the tests run under, it
  silently changed DMC DMA timing and six ROM tests failed on a frame hash.
- **The bisect that found it had to be structural, not by inspection.** Reverting only the
  skip-ahead while keeping the mixing restructure still failed, which is what ruled the restructure
  in and the timers out - and the actual culprit turned out to be neither, but the block-counter
  reset that had been added along the way to fix an unrelated underflow.

### APU: band-limited synthesis

Recorded 2026-07-29, same protocol, interleaved over three rounds.

| build | geomean | delta |
|---|---|---|
| after skip-ahead timers | 2.280 | |
| + band-limited synthesis | 2.303 | **+1.0%** |

At this machine's noise floor, so **treat this as performance-neutral**. The reason to do it is
that it is the anti-aliasing the chain has never had: measured against point-sampling the same
5 kHz square, the alias its 13th harmonic folds to drops from 0.75% of the fundamental to 0.02%.
Level is unchanged (RMS within 1% on every audio test).

**The first cut was +7.7%, and the reason is worth remembering.** The mixer walks from one cycle a
channel could change on to the next, which is only sparse if "could change" is honest. A pulse
whose period register is 0 has a divider that expires every *other* cycle, and a silent game is
full of them - so stepping at every expiry meant 29,776 stops a frame on SMB3, which is every
cycle, which is exactly what the skip-ahead work had removed. Skipping a channel whose output
cannot move - muted, or silenced by its envelope or length counter - took that to **90**. Those
guards are safe because nothing they test can change without a register write or a frame-counter
event, and the walk visits both regardless.

So the shape of the cost is: not the synthesis, not the deltas (15-500 a frame), but how often the
walk stops. Anything added to a channel that makes its output change more often needs a matching
`next_change` guard or this regresses quietly.

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

`perf record -F 999`, `--profile perf`, 3 x 600 frames, `TETANES_BENCH_NO_OUTPUT=1`. Two boards,
because mapper cost is the one thing a single-ROM profile cannot see:

| Function | Zelda (MMC1) | SMB3 (MMC3) |
|---|---|---|
| `Bus::ppu_clock` | 47.6% | 45.3% |
| `Bus::bg_fetch_cycle` | 16.8% | 17.9% |
| `ControlDeck::clock_one_frame` | 12.5% | 9.7% |
| `Ppu::oam_eval_cycle` | 6.8% | 5.6% |
| `Bus::cpu_bus_read` | 4.6% | 3.7% |
| `Apu::clock_lazy` | 3.1% | 2.8% |
| `Bus::fetch_bg_nt_byte` | 2.6% | 2.8% |
| `Mapper::clock` | 2.1% | 1.9% |
| `Apu::clock_channels` | 0.8% | 3.2% |

Rolled up: **PPU ~72-74%, CPU ~17%, APU ~4-6%, mapper ~2-3%.** The whole instruction path -
dispatch, addressing modes and the opcode bodies - inlines into `clock_one_frame`, so that row is
CPU-total rather than one loop; only `bpl` and `lda` stay out of line, and only on SMB3.

Two things this says about where work is worth spending:

- **The APU is done.** It was ~26% of frame time before band-limited synthesis and output-rate
  filtering; at 4-6% it is now cheaper than MesenCE's (~8%), and nothing in it is worth another
  pass. `FilterChain` does not appear at all.
- **The remaining gap to MesenCE is entirely the PPU.** On Zelda, PPU work is 73.8% of 1.923 ms =
  1.42 ms against MesenCE's 67% of 1.600 ms = 1.07 ms. That 0.35 ms difference is the whole 0.32 ms
  frame-time gap; CPU and APU are already at or ahead of parity.

`bg_fetch_cycle` is ~17% even on NROM, where `Nrom::chr_peek` is two match arms - so it is mostly
genuine PPU fetch work, **not** mapper dispatch. Removing dispatch will not reclaim most of it.

Mapper cost still varies enormously by board, which is the whole reason the corpus exists. On
Castlevania III, MMC5-specific code is a large share of frame time - `Exrom::chr_read_hook` runs on
every PPU fetch, and `Exrom::output` on every CPU cycle for expansion audio. On Super Mario Bros.
no mapper symbol appears at all.

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

### Phase 1b — PPU (2026-07-26)

`--profile perf`, 10 iterations x 600 frames, `taskset -c 0`, quiet machine (cv < 0.5% on every ROM
in the table below). `before` is the last commit before this section's changes.

| ROM | Mapper | before | after | delta |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 2.767 | 2.710 | -2.1% |
| Super Mario Bros. | 000 NROM | 2.860 | 2.716 | -5.0% |
| Legend of Zelda | 001 MMC1 | 2.767 | 2.683 | -3.0% |
| Super Mario Bros. 3 | 004 MMC3 | 2.976 | 2.854 | -4.1% |
| Punch-Out!! | 009 MMC2 | 2.702 | 2.613 | -3.3% |
| Castlevania III | 005 MMC5 | 3.849 | 3.783 | -1.7% |
| Akumajou Densetsu | 024 VRC6 | 3.288 | 3.204 | -2.6% |
| **geometric mean** | | **3.008** | **2.914** | **-3.1%** |

Changes, in the order they were made and measured:

1. **Split `Ppu::clock`'s branch chain.** Extracted the scanline-transition path (taken once every
   341 dots) and the PAL-only sprite-eval path into their own `#[cold] #[inline(never)]` functions,
   leaving the render-scanline path (~92% of scanlines) as the only thing inlined into the hot
   function. Pure code motion, no logic changes.
2. **Flag-gate `PpuDebugger`.** The two per-dot `scanline`/`cycle` compares against `self.debugger`
   are pure and side-effect-free, so LLVM was free to (and did) evaluate both unconditionally every
   dot rather than short-circuiting - touching a struct that otherwise sits cold relative to the
   hot per-dot fields. Added a cached `debugger_active: bool`, recomputed only when
   `add_debugger`/`remove_debugger` is called, so the common (no debugger attached) case is a single
   bool check that never touches `debugger`.
3. **Un-box hot arrays - measured, not assumed.** All three of `Ppu::sprites`, `Bus::wram`, and
   `Frame::buffer` were tried un-boxed (removing a pointer chase per access). Results disagreed with
   the "fewer indirections is always faster" prior:
   - `Ppu::sprites` (`[Sprite; 8]`, ~112 bytes, chased per visible pixel by `pixel_palette`):
     un-boxing measured neutral-to-slightly-positive. Kept un-boxed.
   - `Bus::wram` (2 KiB): un-boxing measured **~1.2% slower**, reproducing across repeated back-to-back
     A/B runs. Inlining it grows `Bus`'s footprint enough to outweigh the removed indirection. Kept
     boxed.
   - `Frame::buffer` (120 KiB): un-boxing **overflowed the stack** in
     `control_deck::tests::save_state_resumes_identically`, which moves a `Cpu` (and therefore
     `Bus`/`Ppu`/`Frame`) through several stack frames during a save-state round trip. Kept boxed.
4. **Hoist per-pixel invariants out of `Video::apply_ntsc_filter`.** `NTSC_PALETTE.get_or_init(..)`
   and `even_phase` were recomputed every pixel despite being frame-invariant; the per-pixel
   `(2 + y * 341 + x + even_phase) % 3` was replaced with a rolling counter incremented once per
   pixel and recomputed from scratch only at the start of each row. Verified bit-for-bit identical
   to the original formula via a synthetic regression test
   (`video::tests::ntsc_filter_matches_reference_formula`), since no ROM snapshot test exercises the
   filter. **Invisible to the table above** - `clock_frame()` never calls `apply_filter`, so this
   needed `TETANES_BENCH_OUTPUT=1` to measure at all: 3.162 -> 2.988 ms/frame on the same corpus, a
   further **-5.5%** on top of everything above, on the path real gameplay actually takes.

### Catch-up clocking - measured, deferred until after Phase 6

First pass (below) concluded this wasn't worth pursuing. That conclusion was too narrow and was
corrected after a follow-up measurement - see the plan doc's "PPU/CPU catch-up architecture" entry
under Deferred for the full writeup. Summary: it has a real ~10-12% ceiling (crude periodic-batching
experiment: 3.060 -> 2.740 ms/frame geomean at a batch of 4 cycles, before correctness broke - 26/204
tests failed, every `vbl_nmi_*` test and DMA/IRQ timing among them). Not pursued now because getting
it right requires splitting the PPU into an always-on-every-cycle cheap tick (everything
`handle_interrupts` needs: vblank/A12 edges) and a deferred/batched expensive tick (bg fetch, sprite
eval, pixel write) replayed against a timestamped register-write log - a materially bigger,
correctness-risky change than anything in this phase. Deferred until after Phase 6, tracked with
numbers so it isn't re-litigated from scratch.

Original (incomplete) reasoning, kept for context: considered whether a `Apu::clock_lazy`-style
catch-up would apply to the PPU the way it does the APU, and concluded no because (1) the PPU is
already driven at maximum catch-up granularity - `Cpu::start_cycle`/`end_cycle` call `Ppu::clock_to`
twice per CPU cycle already - and (2) PPU work isn't deferrable the way APU sample synthesis is:
every dot's pixel is written to the frame buffer now or never. Both of those are still true, but
they only rule out *skipping* per-dot rendering work - they say nothing about *how often the CPU and
PPU loops have to cross into each other*, which is the actual lever the measured ceiling comes from.

### Phase 4 — Mapper operation flags (2026-07-27)

`--profile perf`, 10 iterations x 600 frames, `taskset -c 0`, quiet machine. `before` is the last
Phase 1b commit.

| ROM | Mapper | before | after | delta |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 2.710 | 2.712-2.727 | ~flat |
| Super Mario Bros. | 000 NROM | 2.716 | 2.706-2.728 | ~flat |
| Legend of Zelda | 001 MMC1 | 2.683 | 2.694-2.730 | +0.5-1.7% |
| Super Mario Bros. 3 | 004 MMC3 | 2.854 | 2.908-2.920 | +2-2.3% |
| Punch-Out!! | 009 MMC2 | 2.613 | 2.599-2.622 | ~flat |
| Castlevania III | 005 MMC5 | 3.783 | 3.922-3.937 | +3.7-4.1% |
| Akumajou Densetsu | 024 VRC6 | 3.204 | 3.264-3.288 | +1.9-2.6% |
| **geometric mean** | | **2.914** | **2.948-2.956** | **+1.2-1.4%** |

`MapperOps` bitflags (`CLOCKED`, `IRQ`, `AUDIO`, `DMA`) resolved once at cart load and cached beside
the mapper (`Ppu::mapper_ops`), gating `Bus::cpu_clock`'s `mapper.clock()`/`mapper.output()` and
`Cpu::handle_interrupts`'s `mapper.irq_pending()`/`mapper.dma_pending()` - previously unconditional
on every CPU cycle for every board. Audited which of the 22 boards actually need each hook rather
than guessing: 10 need `CLOCKED` (an IRQ or serial-write timing counter, or expansion audio),
9 of those also `IRQ`, 4 `AUDIO` (Exrom/MMC5, Namco163, Vrc6, SunsoftFme7 - matches the plan's
estimate exactly), and only Exrom `DMA`.

Then folded `watches_ppu_bus`/`serves_prg_reads`/`serves_chr_reads` - three more cached bools that
already existed on `Ppu` following the exact same "resolve once at load, gate on a bit test" shape -
into the same `MapperOps` value instead of leaving four near-identical mechanisms side by side.
`Map::mapper_ops()` is now the single source of truth a board implements; the three separate trait
methods are gone, and `Ppu::chr_read`/`chr_peek`/`notify_ppu_bus` and `Bus::read`/`peek` all check
`self.mapper_ops.intersects(MapperOps::X)` instead of a dedicated field.

**This measured slower, not faster or neutral, and both effects are real:**

- The `MapperOps` gating itself (`CLOCKED`/`IRQ`/`AUDIO`/`DMA` only) was a wash on this corpus versus
  the Phase 1b baseline once `Bus::cpu_clock` was marked `#[inline(always)]` - it had regressed ~4%
  with only `#[inline]` (a hint, not a requirement): the extra branches pushed it just over LLVM's
  automatic inlining threshold, so it silently stopped being inlined into the force-inlined
  `Cpu::start_cycle` hot path, turning a free bit test into a real out-of-line call every CPU cycle.
  Confirmed by isolating `bus.rs`'s change from `cpu.rs`'s (each alone was neutral-to-positive; only
  the combination regressed) and then by the `#[inline(always)]` fix recovering it exactly. The
  lesson generalizes: growing a merely-`#[inline]` function that sits behind a force-inlined caller
  is a silent cliff, not a gradual cost - measure after touching anything in the `Cpu::start_cycle`/
  `end_cycle`/`Bus::cpu_clock`/`handle_interrupts` chain.
- Folding the three `watches_ppu_bus`/`serves_prg_reads`/`serves_chr_reads` bools into `MapperOps`
  measured a reproducible **~1.3-1.5% slower**, confirmed across three clean back-to-back runs. The
  likely cost is one extra bitwise AND at call sites that run extremely often - `Ppu::chr_read`/
  `chr_peek` alone run ~41,000 times a frame - though isolating it further (an inlining threshold
  the way `cpu_clock` had, versus `#[repr(C)]` field-order/cache-line perturbation from removing
  three `bool` fields, the same class of surprise `Bus::wram` produced in Phase 1b) wasn't run to
  ground given the size of the win. **Kept anyway**: it replaces four near-identical
  resolve-once-cache-a-bit-test mechanisms with one, which is what "mapper operation flags" in the
  plan actually means, and the corpus here (mostly boards this consolidation makes *cheaper* to
  reason about, not faster) isn't the target audience for the speed side of Phase 4 - that's boards
  that previously paid for `CLOCKED`/`IRQ`/`AUDIO`/`DMA` dispatch they didn't need, which this corpus
  mostly already lacked (NROM/MMC1/MMC2 all had trivial dispatch already; only Namco163/Vrc6/
  SunsoftFme7/BandaiFCG - none in this corpus - previously paid unconditionally for hooks other
  boards now skip).

### Phase 4 — Map trait diet and the `boards!` table (2026-07-27)

`--profile perf`, 10 iterations x 600 frames, `taskset -c 0`, quiet machine (cv < 1.2% throughout).
`before` is the Phase 4 `MapperOps` commit above, re-measured in this session rather than quoted
from the table above.

| ROM | Mapper | before | after | delta |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 2.692 | 2.641 | -1.9% |
| Super Mario Bros. | 000 NROM | 2.731 | 2.683 | -1.8% |
| Legend of Zelda | 001 MMC1 | 2.705 | 2.665 | -1.5% |
| Super Mario Bros. 3 | 004 MMC3 | 2.910 | 2.867 | -1.5% |
| Punch-Out!! | 009 MMC2 | 2.596 | 2.538 | -2.2% |
| Castlevania III | 005 MMC5 | 3.926 | 3.784 | -3.6% |
| Akumajou Densetsu | 024 VRC6 | 3.284 | 3.190 | -2.9% |
| **geometric mean** | | **2.948** | **2.884** | **-2.2%** |

Two refactors, neither of them aimed at speed, measured in sequence:

1. **`Map` gained every method a board needs and lost its supertraits** (2.948 -> 2.915, -1.1%).
   `Map: Clock + Regional + Reset + Sram` cost each of the 22 boards an empty `impl` per trait it
   did not need. `clock`/`reset`/`region`/`set_region` became defaulted `Map` methods instead, and
   `Sram` went entirely - every board's impl was empty, and nothing had called `Mapper::save`/`load`
   since battery PRG-RAM moved into `Memory`. The boards with real `clock`/`reset` bodies moved
   most (MMC5 -3.5%, VRC6 -2.4%, MMC2 -2.2%), consistent with ~12 distinct empty per-board functions
   collapsing into one inlinable trait default.
2. **`boards!` table** (2.915 -> 2.883, a further -1.1%), which also folded `Sample::output` into
   `Map` the same way. `Sample for Mapper` now dispatches over all 22 variants rather than matching
   4 with a `_ => 0.0` fallback, and got *faster* rather than slower - `MapperOps::AUDIO` still gates
   the call, so only the four boards with audio ever reach it.

Together these **more than recover the 1.2-1.4% the `MapperOps` fold cost**, so that consolidation
is now paid for. A re-measurement of the same commit came back at 2.901 rather than 2.883, so read
the total as **-1.6% to -2.2%**; every individual ROM moved the same direction in both runs.

The stable-id serialization change and un-boxing `Fk23C` measured 2.884, i.e. neutral - both are off
the frame path, which is what was expected.

### Enum size vs indirection: which boards still need boxing

Recorded 2026-07-27, on the corpus above **plus two mapper 069 ROMs** (Gimmick!, Batman: Return of
the Joker), added because the first pass at this had no FME7 game to measure and reached the wrong
conclusion without one.

`SunsoftFme7` was the last unboxed board setting `Mapper`'s size: 72 bytes, where every other
unboxed board is <= 56. Boxing it takes `Mapper` to **56 bytes**.

| ROM | Mapper | unboxed | boxed | delta |
|---|---|---|---|---|
| spritecans | 000 NROM | 2.659 | 2.621 | -1.4% |
| Super Mario Bros. | 000 NROM | 2.675 | 2.646 | -1.1% |
| Legend of Zelda | 001 MMC1 | 2.689 | 2.602 | -3.2% |
| Super Mario Bros. 3 | 004 MMC3 | 2.884 | 2.827 | -2.0% |
| Punch-Out!! | 009 MMC2 | 2.568 | 2.499 | -2.7% |
| Castlevania III | 005 MMC5 | 3.849 | 3.749 | -2.6% |
| Akumajou Densetsu | 024 VRC6 | 3.232 | 3.117 | -3.6% |
| **Gimmick!** | **069 FME7** | **3.385** | **3.350** | **-1.0%** |
| **Batman: Return of the Joker** | **069 FME7** | **3.218** | **3.163** | **-1.7%** |
| **geometric mean** | | **2.992** | **2.927** | **-2.2%** |

**Applied.** The interesting row is FME7 itself: the board that *pays* the new indirection, on a
struct whose audio is clocked every CPU cycle, still came out faster. Shrinking `Ppu`'s inline
`mapper` field outweighs the pointer chase even for the board being chased. An earlier pass over a
corpus with no FME7 ROM measured the other eight ROMs improving and declined to apply the change,
reasoning that the cost side was unmeasured — the cost side turned out not to exist.

**The general lesson is that boxing is a measured trade, not a size rule, and it has now surprised
us in both directions**: un-boxing `Bus::wram` (2 KiB) measured 1.2% *slower* despite removing an
indirection, and boxing `SunsoftFme7` (72 bytes) measured 2.2% *faster* despite adding one. Neither
is predictable from the struct size alone; both are about what else fits in cache alongside.

`Fk23C` went the other way and was **un-boxed**: boxed back when it was 280 bytes, the page-table
port left it at 56, so with FME7 boxed it now sets the enum's size on its own and the box bought
nothing but an allocation. Measured neutral, as expected for something off the frame path.

### Trait removal, and a methodology trap (2026-07-27)

Removing the nine convention-only traits (`Clock`, `Reset`, `Regional`, `Sample`, `Sram`,
`TimerCycle`, `Consume`, `InputRegisters`, `PpuAddr`) in favour of inherent methods measured
**2.921 vs 2.927 ms/frame geomean — neutral**, which is the expected answer: static dispatch either
way, same code, fewer imports.

Getting to that answer took two corrections worth recording.

**1. Don't expand a hot helper into its call sites.** `Apu::channel_clock_to` had a
`clock_to<T: Clock + TimerCycle + Sample>` helper, the only generic use of any of these traits. The
obvious replacement was a macro expanding the loop body into each of the five match arms — that
measured **+2.2%**, because it turns one small function into five copies of a loop and stops
`channel_clock_to` being a sensible inlining candidate. Emitting one *monomorphic function per
channel type* instead — exactly what the generic function had produced — recovered it and then some.
**Keep the call boundary a generic function would have created.**

**2. Discard the first run after a rebuild.** The same commit measured **2.927 in
`/home/luke/dev/tetanes` and 3.024 in a worktree under `/tmp/...`**, and this was first written up
here as a "3.3% effect from build location". **That was wrong** - see "The first run after a compile"
below for the controlled experiment and the actual cause. The bisect below is still valid, because
every point in it was measured the same way:

| State (each a first run after its rebuild) | geomean |
|---|---|
| before trait removal | 3.024 |
| trait removal, macro expanded into call sites | 3.091 (+2.2%) |
| trait removal, one monomorphic fn per channel | 2.999 (-0.8%) |

### Phase 5 — save states carry only the mutable tail (2026-07-27)

`Memory` is one contiguous allocation with the immutable regions (PRG-ROM, CHR-ROM) placed first and
`ram_start` marking where the mutable tail begins. That layout was built for this, and the field's
own comment already said "save states only need `data[ram_start..]`" — but `Memory` still derived
`Serialize`, so **every save state and every rewind snapshot carried a verbatim copy of the cart's
ROM**.

Hand-written `Serialize`/`Deserialize` now store the layout plus the RAM tail only, and
`Cpu::load` — the single funnel every restore path goes through; since the ownership flattening it
is `Bus::load_state` — copies the ROM back in from the console already running.

| ROM | state before | state after | change |
|---|---|---|---|
| spritecans (000, 32K PRG) | 48,547 B | 22,955 B | -53% |
| Castlevania III (005, 256K+128K) | 474,945 B | 80,713 B | -83% |
| Super Mario Bros. 3 (004, 384K) | 417,216 B | 22,984 B | **-94.5%** |

The time cost falls with the size, and deflate — which dominates writing a state to disk — falls
fastest, because it was compressing hundreds of KiB of ROM every time:

| Operation (SMB3) | before | after | change |
|---|---|---|---|
| bincode encode | 0.213 ms | 0.011 ms | **-95%** |
| bincode decode | 0.446 ms | 0.040 ms | **-91%** |
| deflate | 13.833 ms | 0.096 ms | **-99.3%** |

Three paths benefit, only one of which is the visible "save state" feature:

- **Rewind** keeps `60 * seconds / interval` snapshots **uncompressed in RAM** — 900 at the default
  30 s / 2 frames. For SMB3 that buffer goes from **~375 MB to ~21 MB**.
- **Run-ahead** encodes *and* decodes the whole console every frame. For SMB3 that is 0.659 ms of a
  ~2.9 ms frame before, and 0.051 ms after — **~20% of frame time returned** to anyone using it.
- **Save/load state** stops spending 14-18 ms in deflate.

`clock_frame` itself is untouched by this and measured **2.891 vs 2.909 neutral** in a like-for-like
A/B. The main checkout showed +2.3% for the same change, which is the first-run-after-rebuild effect
described below - worth stressing that it reproduces across repeated runs of the *same* binary, and
so looks convincing on its own.


### Run-ahead snapshots: clone, don't serialize

Once ROM was out of the save state, the reason run-ahead serialized at all was gone. It snapshots
the console, clocks `run_ahead` frames to produce the displayed frame, then rewinds to the snapshot -
all within one frame, in the same session. A `Cpu::clone` does that directly.

Snapshot + restore, measured per call:

| ROM | bincode round trip | clone | speedup |
|---|---|---|---|
| spritecans (000) | 0.0492 ms | 0.0203 ms | 2.4x |
| Super Mario Bros. 3 (004) | 0.0670 ms | 0.0310 ms | 2.2x |
| Castlevania III (005) | 0.1715 ms | 0.0333 ms | **5.1x** |
| Legend of Zelda (001) | 0.1139 ms | 0.0724 ms | 1.6x |

The decode side is what the clone avoids: it has to allocate and zero the whole `Memory` arena and
then copy the ROM back into it, where the clone is one `memcpy` of an allocation it makes once. The
clone also carries the page tables, so the restore no longer needs `sync_mapper`.

Combined with the ROM removal, run-ahead's per-frame overhead on SMB3 went **0.659 ms -> 0.031 ms,
21x**, against a ~2.9 ms frame.

**Rewind keeps the serialized form**, and should: it holds ~900 snapshots in RAM simultaneously, so
cloning each would put the hundreds of megabytes straight back. The two paths look similar and want
opposite representations - run-ahead optimizes for round-trip latency of one live snapshot, rewind
for the size of many cold ones.


### The first run after a compile

Twice a change measured several percent slower than it should have, and twice the explanation
offered here was wrong ("code layout shifted with the source path", "tmpfs"). The controlled
experiment, same commit built from three worktrees varying path length and filesystem:

| Directory | length | filesystem | geomean |
|---|---|---|---|
| `/home/luke/dev/BTRF` | 19 | btrfs | 3.010 |
| `/tmp/aaaaaaaaaaTMPF` | 19 | tmpfs | 3.010 |
| `/home/luke/dev/BTRFSMUCHLONGERPATHXXXXXXXXXX` | 44 | btrfs | 2.995 |

**All three produced the identical binary** - same cargo fingerprint hash
(`clock_frame-fd0a03be75b32552`), same size, same output path. `CARGO_TARGET_DIR` is a shared
`~/.cache/cargo-target`, so a build never lands on tmpfs no matter where the source lives, and the
benchmark loads its ROM *outside* the timed region, so source-tree I/O cannot reach the measurement
either. **There is no directory effect and no filesystem effect.**

What there is:

| Same binary, same directory, no source change between runs | geomean |
|---|---|
| run 1, immediately after a rebuild | **2.998** |
| run 2 | 2.953 |
| run 3 | 2.964 |
| run 4 | 2.959 |

**The first run after a compile is ~1.5% slow**, settling to ~0.4% spread after that. A `cargo bench`
that has just rebuilt had eight compiler threads saturating the machine moments earlier, so its first
measurement is taken in a different thermal/frequency state.

That accounts for the whole "3.3% directory difference": the main-checkout number was a repeat run,
and the worktree number was a first run after the rebuild that switching commits forced.

**So: run the benchmark twice and use the second.** Every A/B in this file stands, because both sides
of each comparison were measured the same way - but the explanation previously given for the
discrepancy did not.

### The APU filter chain, two rates instead of six (2026-07-28)

`FilterChain::consume` runs once per CPU cycle - ~1.79 M/s - and was the second largest line item
in the profile at 13.1%. It walked all six `SampledFilter` entries every call, comparing and
advancing each one's own `period_counter`, with the filter data inline between them.

Five of those six counters were doing one counter's work. Stage 1's period is exactly `dt`, so it
fires every cycle; stages 2-5 are all constructed at the *same* intermediate rate, so their
counters hold identical values for the life of the chain and always fire together. The rewrite
keeps one counter for the intermediate rate, calls stage 1 unconditionally, and drops the `Filter`
enum, `SampledFilter` and `FilterKind::Identity` with it.

Interleaved A/B, `--profile perf`, `taskset -c 0`, first round of each session discarded, and any
ROM reporting above 2% cv dropped as the methodology above requires (three measurements out of 84):

| ROM | Mapper | before | after | delta |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 2.566 | 2.446 | -4.7% |
| Super Mario Bros. | 000 NROM | 2.610 | 2.536 | -2.8% |
| Legend of Zelda | 001 MMC1 | 2.593 | 2.524 | -2.6% |
| Super Mario Bros. 3 | 004 MMC3 | 2.854 | 2.759 | -3.3% |
| Punch-Out!! | 009 MMC2 | 2.544 | 2.445 | -3.9% |
| Castlevania III | 005 MMC5 | 3.741 | 3.710 | -0.8% |
| Akumajou Densetsu | 024 VRC6 | 3.147 | 3.055 | -2.9% |
| **geometric mean** | | **2.838** | **2.753** | **-3.0%** |

Six interleaved pairs at 3 iterations. A second session of three pairs at 5 iterations, run first,
gave **-3.9%** (2.865 -> 2.752), and with the filter on (`frame_buffer` in the timed loop) **-2.2%**
(2.882 -> 2.818). **All 21 ROM-level comparisons across the three configurations moved the same
direction**, which is what carries the result - this machine was unusually noisy that day, the
*same* build ranging 2.801 to 2.932 across rounds, so no single pair is worth much on its own.

The absolute saving is ~0.09 ms/frame and is roughly constant across boards, which is what a change
to a per-CPU-cycle cost that every board pays equally should look like. That is why MMC5 moves least
in percentage terms: the same saving against a frame 30% longer.

Verified bit-identical to the six-counter chain before landing - 3 regions x 2 output rates x 60k
cycles compared on `f32::to_bits` - and the check was negative-tested: dropping the one-sample
warmup quirk (the old stage-1 counter starts empty, so it skips its first call) diverges at cycle
19 and never reconverges, since the stages are recursive. The reference chain was deleted once it
had done that job; it is in the history of this commit if it is ever needed again.

### Baking the NTSC palette at build time (same commit)

`generate_ntsc_palette` cost **~34 ms** of `powf` and `sin_cos`, paid lazily by whichever frame
first used the NTSC filter. It showed up in the profile as 0.52% of a 6.5 s run - i.e. entirely
one-time, but landing as a hitch the moment the filter is switched on. `build.rs` now computes it.

Frame time is unaffected and cannot be otherwise: the benchmark's 120 untimed warmup frames absorb
the init even in the old build. The pixel loop did change - the table is stored as RGB triples
rather than `u32`, so the loop copies three bytes instead of loading four and shifting - and that
measured neutral, inside the noise of the numbers above.

The cost is binary size. Measured on the bench binary: `.rodata` +294,560 B for the table, `.text`
-5,062 B for the generator, **net +289 KB**.

| the 288 KiB table | bytes |
|---|---|
| raw | 294,912 |
| gzip -9 | 166,038 |
| brotli -11 | 117,397 |

Worth knowing for the wasm build: **do not pre-compress the blob in the binary**. A deflated table
embedded in the wasm is ~166 KB over the wire, where shipping it raw and letting the transport
compress is ~117 KB - the transport does a better job than an inner layer it can no longer squeeze.

### The PPU dot loop is not branch-bound (2026-08-01)

Reading MesenCE's `NesPpu::Exec` next to `Bus::ppu_clock` says the dot loop should be branch-bound:
MesenCE reaches its scanline dispatch in two predicted branches and hides every deferred update
behind one `_needStateUpdate` bool, where `ppu_clock` tests `mask.clock()`, `scroll.delayed_update`
and a handful of ranges on each of ~89,000 dots a frame. **It is not.**

Merging the pixel and shift-register range tests into one, and putting the `cycle == 1` compare
ahead of the two vblank scanline compares, removes about four operations per dot and measured
**-0.7%** across five interleaved rounds (-0.97, -0.65, +0.19, -0.05, -1.94) - straddling the noise
floor. Kept, because the code is shorter, not because the number is convincing. The third change
this ranking predicted, collapsing the two deferred-update tests behind a single cached bool, was
not attempted once the first two came back this small.

The lesson generalises: **these branches are perfectly predicted, and a perfectly predicted branch
on an out-of-order core is close to free.** Counting operations in a hot loop ranks work in the
order a static reading suggests, not the order the machine experiences.

> **`perf annotate -s <symbol>` reports percentages of that symbol, not of the program.** Since
> `ppu_clock` is ~47% of the frame, every number it prints has to be halved before it means
> anything next to a `perf report` share. Use `perf report --sort srcline` for line shares that are
> already whole-program, and `-e cycles:pp` so PEBS removes skid. On this workload precise and
> non-precise sampling agree to within a few tenths, so skid is not what misleads here - reading
> symbol-relative percentages as absolute ones is.

Whole-program line shares, Zelda, `cycles:pp` (lines at or above 0.3% cover 84% of samples):

| Source | Share of frame |
|---|---|
| `Ppu::load_sprites` (`ppu.rs:1455-1480`) | 10.0% |
| `Ppu::pixel_palette` body (`ppu.rs:910-975`) | 8.7% |
| `PaletteRam::peek` + `mirror` (`ppu.rs:82,90`) | 5.4% |
| `Ppu::render_pixel` body (`ppu.rs:1024-1035`) | 4.7% |
| `Memory::prg_peek`/`chr_peek` (`memory.rs:324,325,340,341`) | 4.1% |
| `Frame::set_pixel` (`frame.rs:87`) | 2.2% |
| `Bus::check_debugger` (`ppu.rs:1660-1662`) | 1.0% |

### Palette mirroring: read-side table beats write-side duplication

`PaletteRam::peek` resolves the $3F10/$3F14/$3F18/$3F1C backdrop mirrors through a 32-byte table,
so a pixel pays two loads where the second depends on the first's result - 61,440 times a frame,
with the result feeding `set_pixel` immediately. MesenCE pays one load, because it writes both
halves of each mirror and indexes storage directly.

Duplicating on write and indexing directly - which is exactly what MesenCE does, at
`BaseNesPpu::WritePaletteRam` - measured **+1.8% slower** (2.158 against 2.120, three consecutive
rounds, no round disagreeing). Every ROM snapshot hash was unchanged, so this is a pure performance
answer. Reverted; the comment on `PaletteRam::mirror` records the measurement.

Why so little was on the table: the whole pair is 5.4% of the frame, the second load is the one
that has to happen, and dropping the first shortens a chain whose consumer - a store into the frame
buffer - nothing is waiting on. What is left is well inside the range a code-layout shift moves a
function that is 47% of runtime, which is the likeliest explanation for the sign.

**Neither load is bounds checked.** `ConstArray` indexes with `index & (N - 1)`, so the disassembly
is `and $0x1f` and a `movzbl`, with no compare and no panic edge - `get_unchecked` has nothing to
remove here.

### Sprite coverage as a bitmask (2026-08-01)

The one change in this round that paid, and the one that removes *work* rather than a branch.

`Ppu::spr_cover` holds one bit per sprite index for each dot, so `Ppu::pixel_palette` visits only
the sprites whose 8-pixel span contains the dot - `trailing_zeros` for the next set bit, `n & (n-1)`
to clear it, which is also sprite priority order. Scanning all `spr_count` sprites and range-testing
each is what it replaces, and those range tests are data-dependent on sprite X positions, so they
mispredict in a way the dot loop's fixed branches never do.

| ROM | Mapper | before | after | delta |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 1.880 | 1.783 | -5.2% |
| Super Mario Bros. | 000 NROM | 1.852 | 1.830 | -1.2% |
| Legend of Zelda | 001 MMC1 | 1.945 | 1.832 | -5.8% |
| Super Mario Bros. 3 | 004 MMC3 | 2.077 | 2.006 | -3.4% |
| Punch-Out!! | 009 MMC2 | 1.815 | 1.703 | -6.2% |
| Castlevania III | 005 MMC5 | 3.060 | 2.915 | -4.7% |
| Akumajou Densetsu | 024 VRC6 | 2.522 | 2.408 | -4.5% |
| **geometric mean** | | **2.127** | **2.032** | **-4.5%** |

The span test inside the loop stays, and is not redundant: the cover bits are rebuilt at dot 257
only when rendering was enabled there, so toggling `$2001` mid-frame can leave a bit set against a
sprite that has since moved. Without the test that underflows `7 - spr_shift`, which
`ppu::spr_hit_right_edge` and `ppu::read_buffer` both catch. Keeping it costs nothing measurable -
it is a branch that is taken almost every time the bit is set.

Cost is one byte per dot instead of one bit, for a 256-byte array that was already there.

### What a bounds check actually costs

The two hot reads that *are* bounds checked - `Memory::chr_peek`, whose arena is a `Box<[u8]>` the
compiler cannot prove an index into, and `Frame::set_pixel`, whose `Buffer` derefs to a plain array
rather than through `ConstArray`'s mask - both show the check in the disassembly and both show it
costing nothing.

`Frame::set_pixel`, from `perf annotate -e cycles:pp`:

```
shl  $0x8,%edi        1.67
add  %rax,%rdi        0.00
cmp  $0xf000,%rdi     0.00      <- the bounds check
jae  <panic>          0.00      <- the panic edge
```

`Memory::chr_peek`:

```
mov  0x370(%rdi),%rsi   0.11    <- load data.len()
cmp  %rsi,%rax          0.38    <- the bounds check
jae  <panic>            0.00
mov  (%r14),%rdx        0.00
movzbl (%rdx,%rax,1)    1.75    <- the load the whole thing exists to do
```

So **`get_unchecked` is worth at most ~0.5%, and only on `chr_peek`** - the length load and the
compare. The branch to the panic block is 0.00% in both: never taken, perfectly predicted, and the
block itself is laid out cold so it costs no instruction cache on the hot path. This is the whole
reason the planned "remove the remaining bounds checks" work was dropped rather than attempted.

### Greyscale and emphasis, folded in by the run (2026-08-01)

`$2001`'s greyscale bit and three emphasis bits are a mask and an or on the colour of every pixel -
two operations, 61,440 times a frame, to apply settings that the overwhelming majority of frames
never turn on at all.

`Ppu::apply_color_bits` folds them into whole runs of pixels instead: rendering stores the raw
palette colour, and the bits are applied over everything drawn since the last run whenever `$2001`
is about to change and once at the end of each frame. When neither is set, which is the usual case,
the pass is a bounds compare and an assignment.

| ROM | Mapper | before | after | delta |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 1.785 | 1.768 | -1.0% |
| Super Mario Bros. | 000 NROM | 1.830 | 1.783 | -2.6% |
| Legend of Zelda | 001 MMC1 | 1.835 | 1.788 | -2.6% |
| Super Mario Bros. 3 | 004 MMC3 | 2.015 | 1.991 | -1.2% |
| Punch-Out!! | 009 MMC2 | 1.716 | 1.725 | +0.5% |
| Castlevania III | 005 MMC5 | 2.913 | 2.853 | -2.1% |
| Akumajou Densetsu | 024 VRC6 | 2.414 | 2.372 | -1.7% |
| **geometric mean** | | **2.038** | **2.007** | **-1.5%** |

**Reset the mark when the visible frame starts, not when it ends.** Leaving it at the end of the
buffer through vblank is what makes a `$2001` write there - which games do every frame - find
nothing outstanding instead of folding the bits into the finished frame a second time. Getting this
backwards fails `apu::dpcmletterbox` and nothing else, which is a thin margin for a bug that
double-darkens every frame of any game using emphasis.

### Drilling into a symbol that everything inlines into

`Bus::ppu_clock` is ~45% of frame time and almost the whole PPU is inlined into it, so the function
list says nothing about what inside it is expensive. Two traps in getting at that:

- **`perf annotate -s <symbol>` percentages are relative to that symbol.** They sum to 100% of
  `ppu_clock`, not of the run, so every figure needs multiplying by the symbol's own share before it
  can sit next to a `perf report` number. `--percent-type global-period` does *not* fix the sorted
  summary - it only affects the disassembly listing below it.
- **Line numbers alone are not function names.** A range read off the summary can span two inlined
  functions and be attributed to the wrong one.

What works is `perf report --no-children --sort srcline -e cycles:pp`, whose shares are
whole-program already, with each line mapped to its enclosing `fn` by matching braces in the source.
Zelda, current:

| Inlined function | Share of frame |
|---|---|
| `Ppu::pixel_palette` | 10.8% |
| `Bus::bg_fetch_cycle` | 9.7% |
| `Bus::ppu_clock` itself | 8.8% |
| `Ppu::oam_eval_cycle` | 4.9% |
| `Bus::clock_render_scanline` | 4.4% |
| `Ppu::render_pixel` | 3.7% |
| `Memory::chr_peek` | 3.6% |
| `Bus::ppu_clock_to` | 3.5% |
| `PaletteRam::mirror` + `peek` | 6.1% |
| `Bus::cpu_bus_read` | 2.5% |
| `Frame::set_pixel` | 2.1% |

### Cache-line placement past the first line is not predictable

`Ppu` is `#[repr(C)]` and hand-ordered so the per-dot working set - counters, `mask`, `ctrl`,
`scroll`, the tile shifters, the scanline-kind flags - fills the first 64 bytes exactly. That part
is real and `tests::print_layouts` now asserts it.

Past that, offset arithmetic stops predicting anything. `palette` is 32 bytes at offset 104, so it
straddles the line at 128, and it is read once per visible pixel - textbook grounds for moving the
eight bytes above it. Doing that lands `palette` at 96 inside a single line and incidentally aligns
`spr_cover` to 256 and `oamdata` to 512, and it measured **3.2% slower** (2.072 against 2.008, three
rounds). The eight bytes cannot move without shifting every field below them, and the layout that
looks worse on paper is the one that runs faster. Left as it is.

**`print_layouts` had been printing a field it was not measuring.** `use super::prelude::*` brings
`video::Frame` into scope, so the `frame: Frame` row was reporting the size of the RGB output frame
rather than `ppu::frame::Frame`, and `color_bits_applied` was missing entirely. A test that only
prints cannot notice either. It now asserts the first cache line, which is the part that is load
bearing.

There is no third-party crate worth adding for this - `std::mem::offset_of!` is what the macro
already uses, and it is the whole mechanism.

### Precomputed draw thresholds, and what the first cache line is worth (2026-08-01)

`Ppu::pixel_palette` asked, for every pixel, whether the background is enabled and whether this dot
is past the left-column clip - four flag loads and three logic operations, from settings that only
change on a `$2001` write. `Mask::min_draw_bg_cycle` and `min_draw_spr_cycle` hold the first dot
each layer is drawn on, or [`Mask::NEVER_DRAWN`] (300, past the end of a scanline) when it is not,
so the pixel path compares once.

| ROM | Mapper | before | after | delta |
|---|---|---|---|---|
| spritecans | 000 NROM (sprite stress) | 1.755 | 1.716 | -2.2% |
| Super Mario Bros. | 000 NROM | 1.792 | 1.742 | -2.8% |
| Legend of Zelda | 001 MMC1 | 1.808 | 1.774 | -1.9% |
| Super Mario Bros. 3 | 004 MMC3 | 1.996 | 1.959 | -1.9% |
| Punch-Out!! | 009 MMC2 | 1.716 | 1.672 | -2.6% |
| Castlevania III | 005 MMC5 | 2.857 | 2.808 | -1.7% |
| Akumajou Densetsu | 024 VRC6 | 2.356 | 2.323 | -1.4% |
| **geometric mean** | | **2.007** | **1.965** | **-2.1%** |

**The two thresholds only pay if they are free.** Adding them as two `u16` fields grew `Mask` from
12 to 16 bytes, which pushed `is_render_scanline` from offset 63 to 67 and spilled the per-dot
working set out of the first cache line. Measured that way the same logic change was **+1.0%
slower**. Dropping the four `show_*` bools it made redundant - they are decoded from `bits` on
demand now, and only a debugger path still asks - put `Mask` back at 12 bytes and the line back at
exactly 64, and the same change became -2.1%.

**A 3.1% swing from four bytes.** Alongside the 3.2% regression from realigning `palette` (above),
the two together say something narrow and useful: the first 64 bytes of `Ppu` are worth defending
and `tests::print_layouts` should keep failing when they overflow, while placement past that line
is not something to reason about at all - only to measure. A change that adds a hot field is really
two changes, and the layout half can be larger than the logic half and point the other way.

### The registers are fields on `Ppu`, not structs (2026-08-01)

`$2000`, `$2001` and `$2002` are `ctrl_*`, `mask_*` and `status_*` fields on [`Ppu`], with each
register's logic in an `impl Ppu` block in its own module - the same arrangement `Bus` already uses,
where a CPU access lives with the state it reads. One definition site per register, field order
alone decides cache placement, and `Mask` no longer keeps a second copy of `NesRegion`.

`status_bits` and `ctrl_bits` are gone with the structs. Neither register has readable content the
flags do not already carry: `$2000` is write-only, and `$2002`'s low five bits are open bus, so
`Ppu::read_status_bits` composes the byte from the three flags. That also takes a bit-set out of
the pixel loop.

**What this cost, and why the answer took four tries to find.** Flattening measured **+2.4%**, and
neither restoring the old field offsets nor removing the `*_bits` fields recovered any of it.
`perf stat` is what settled it:

| counter | structs | flat |
|---|---|---|
| cycles | 18.088 G | 18.665 G (**+3.2%**) |
| instructions | 44.690 G | 44.737 G (+0.10%) |
| branches | 8.245 G | 8.250 G (+0.06%) |
| branch-misses | 136.8 M | 138.0 M (+0.90%) |
| L1-dcache-load-misses | 5.567 M | 5.487 M (-1.4%) |
| cache-misses | 287.9 K | 224.3 K (-22%) |
| `idq.dsb_uops` | 35.91 G | 27.82 G (**-22.5%**) |
| `idq.mite_uops` | 15.20 G | 22.60 G (**+48.7%**) |

Same instructions, same branches, *better* data-cache behaviour, 3.2% more cycles. Uop-cache
delivery fell from 70.3% to 55.2% of the stream: about 8 billion uops stopped coming from the DSB
and went through legacy decode instead. Disassembling `ppu_clock` in both builds confirmed it was
nothing in the code - 988 instructions against 996, with the differences (`orb` -3, `andb` -1) in
the flat build's favour, and function start alignment identical in both.

The fix was to shrink what `ppu_clock` has to keep in the uop cache. `Ppu::load_sprites` runs on 8
of every 341 dots for 0.12% of frame time, and inlined it was **more than half** of `ppu_clock`'s
4121 bytes; `Ppu::headless_sprite_zero_hit` only runs under `HeadlessMode` at all. Marking both
`#[inline(never)]` took `ppu_clock` to 2252 bytes and the flat build to **-0.6%** against the
struct version.

**Outlining is not a general win** - the same two attributes on the struct build measured **+4.0%
slower**. Like every layout result here it is a draw, not a rule. What is durable is the diagnosis:
when cycles move but instructions, branches and cache misses do not, look at `idq.dsb_uops` versus
`idq.mite_uops` before looking anywhere else, and treat the hot function's byte count as the thing
to manage.

`RUSTFLAGS="-Cllvm-args=-align-all-nofallthru-blocks=5"` also recovers it - measured 1.944 against
1.971 - but it grows `.text` by 22% (714 KB to 874 KB), matters differently for the wasm build, and
rides on an unstable interface. It stays a measurement instrument, not a shipped flag: building a
variant both ways is the cheapest way to tell a real change from a lucky draw.

### Measure every candidate at two code layouts

Since a code-layout draw is worth ±3% here, a single A/B cannot tell a real change from a lucky
one. Build each variant twice - once normally, once with
`RUSTFLAGS="-Zthreads=8 -Cllvm-args=-align-all-nofallthru-blocks=5"` - and believe the sign only
when both agree.

The second layout is owed only to decisions the perf number itself makes (2026-08-03): a change
accepted *because* it wins, or rejected *despite* being wanted, gets both layouts, because those
are the two verdicts a layout draw can corrupt. A fix or refactor kept on its own merits gets a
single default-layout run to rule out a large regression, and a clean sub-noise-floor null at
default stands without confirmation. The aligned build never ships either way -
basic-block placement has no source-level annotation to pin it, which is exactly why it works as
an independent second sample of the layout lottery.

Candidates measured across both layouts:

| candidate | default codegen | block-aligned | verdict |
|---|---|---|---|
| palette mirrors resolved at write time | +4.1% | +0.9% | consistently slower, rejected |
| horizontal flip baked into the stored sprite bytes | +4.3% | +0.2% | slower, rejected |
| BG shifter reload dropped from the garbage/dummy fetches | +0.2% | -0.3% | neutral, kept as the base of a palette-latch fix |
| `bg_fetch_cycle` dispatch folded into a single `match` | +1.5% | -1.5% | perfectly symmetric layout draw, rejected |
| cached `Scroll::fine_y` deleted, single-store `increment_y` | **-2.2%** | **-1.4%** | real win, kept |

The `fine_y` result (2026-08-03) is the counterpart to the branch results: the dot loop does not
care about predicted branches, but it does care about stores. Deleting the derived field removed
a second store from every `v` mutation (up to six per `increment_y`), and both layouts agreed on
the sign and cleared the floor.

Under the narrowed rule, the dummy-fetch trim (reads on 337/339 only, an accuracy fix) measured
+0.5% at default only - sub-floor, kept on correctness grounds.

The reload result (2026-08-03) is also a layout cautionary tale: the intermediate shape - reload
split out but still pinned to the dummy fetches - measured **+2.8% at default codegen and +0.15%
aligned**. One more call pair at one site was worth almost 3% at one layout and nothing at the
other. The final shape erased it. Twelve removed reloads per scanline were themselves worth
nothing measurable, which is the "not branch-bound" finding again at a second site: the reload
stores all hit L1 lines the fetch path already owns.

The palette result settles a question an earlier single-layout run left open: read-side mirroring
really is faster, not accidentally faster.

The flip result is arithmetic, not layout. Storing sprite tile bytes already flipped removes one
conditional select per covering sprite pixel - about 15,000 a frame - but costs a `reverse_bits` on
two bytes per sprite load, and x86 has no bit-reverse instruction, so that is roughly seven
operations 3,840 times a frame. MesenCE gets this for free because its sprites are genuine shift
registers, clocked one bit per dot, so the flip is just which way the byte was loaded and the pixel
is always the top bit - no variable shift and no select. Porting that means giving the sprite
pipeline per-dot shift state, which is a different design rather than a tweak.

Read in full (2026-08-03), that port was declined. MesenCE's flip is not free after all -
`LoadSprite` bit-reverses both tile bytes of a mirrored sprite at load, the same mechanism the
rejected experiment above measured - and its active-shifter walk has no early exit: it must visit
every active shifter on every dot to clock it, where the cover-mask walk stops at the first opaque
pixel. What remains in its favor is small: fixed top-bit extraction instead of a flip select plus
a variable shift per covering pixel, and one activation compare per dot instead of a cover-array
load per pixel - paid for with a per-scanline sort of the shifter list and extra state exactly at
the accuracy-sensitive edges (odd-frame skip, mid-frame rendering toggles). The expected payoff
sits inside the noise floor, and this session's pattern was that candidates argued from saved
operations measured null while the one that removed stores (`fine_y`) won. The sprite pipeline
stays as it is until a profile shows the pixel walk itself, not its arithmetic, on top.

### `-C target-cpu` is worth about 1% (not shipped)

`RUSTFLAGS="-Zthreads=8 -Ctarget-cpu=x86-64-v3"` measured **-1.1%** (2.017 against 2.039 geomean,
three interleaved rounds, none disagreeing). AVX2/BMI instruction count in the bench binary goes
from 54 to 517, so the flag is doing something; it is just not doing much, because this workload is
byte-at-a-time state machine code with nothing to vectorise.

Two things to know before reaching for it:

- **Pass it as `RUSTFLAGS` and keep `-Zthreads=8`,** or the env var replaces the workspace cargo
  config's `build.rustflags` wholesale and drops the parallel frontend.
- **Read the binary path back from `cargo build --message-format=json`.** Changing rustflags changes
  the `-C metadata` hash, so the artifact lands under a different filename and copying the old path
  silently benchmarks the previous build against itself.

It stays out of shipped builds: MesenCE passes no `-march` either, so leaving it off keeps the
comparison architectural, and v3 excludes pre-2015 hardware for ~1%. If hardware FMA ever becomes a
baseline assumption, `Apu::mix_level` is where it would pay - the comment there explains why
`mul_add` is avoided today.
