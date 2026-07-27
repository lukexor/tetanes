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
the NTSC filter are **not** measured by default. Set `TETANES_BENCH_OUTPUT=1` to switch to
`clock_frame_output()` and include it - needed to see any change to `Video::apply_ntsc_filter` or
`Video::decode_buffer`, neither of which `clock_frame()` ever calls.

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

`Mapper` is 72 bytes, and `SunsoftFme7` (72) is what sets that - every other unboxed board is <= 34.
Boxing it would take `Mapper` to **56 bytes**, the only remaining lever on the enum's size.

Measured on the four-ROM subset that has no FME7 game in it: spritecans 2.641 -> 2.602, SMB3
2.867 -> 2.855, Castlevania III 3.784 -> 3.791, Akumajou 3.190 -> 3.139. Small and mostly
favourable, i.e. shrinking `Ppu`'s inline `mapper` field helps ROMs that never touch the board.

**Not applied.** The corpus has no mapper 069 ROM (the library surveyed for this has none), so the
cost side - an indirection on a board whose audio is clocked every CPU cycle - is unmeasured, and
this is precisely the shape of change that surprised us before: un-boxing `Bus::wram` measured 1.2%
*slower* despite removing an indirection. Revisit with an FME7 ROM (Gimmick!, Batman: Return of the
Joker) in the corpus.

`Fk23C` *was* un-boxed: boxed back when it was 280 bytes, the page-table port left it at 56, below
`SunsoftFme7`'s 72, so `Mapper` is 72 either way and the box bought nothing.
