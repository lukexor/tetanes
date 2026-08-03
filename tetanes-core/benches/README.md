# Benchmarks

There is one benchmark, `clock_frame`. It times `ControlDeck::clock_frame` over a corpus of ROMs and
reports per-ROM mean frame time in milliseconds, with standard deviation, coefficient of variation,
min and max, plus a geometric and arithmetic mean across the corpus.

Frame time is the number that matters: 60fps needs a frame in under 16.6 ms, and everything else the
emulator does happens in whatever is left.

## Running

```sh
cargo make bench                                    # spritecans.nes only (committed)
cargo make bench -- path/to/rom.nes                 # one specific ROM
TETANES_BENCH_ROMS=path/to/roms cargo make bench    # every .nes in a directory
TETANES_BENCH_ROMS="a.nes:b.nes" cargo make bench   # an explicit list
```

`cargo make bench` wraps the run in `perf stat` (cycles, instructions, cache and branch counters)
pinned to core 0 with `taskset`. The plain form is quieter and is what an A/B wants:

```sh
cargo bench --profile perf --bench clock_frame -- path/to/rom.nes
```

Profiling:

```sh
cargo make flamegraph -- path/to/rom.nes    # -> target/flamegraph.svg
cargo make perf-report -- path/to/rom.nes   # flat perf profile to stdout
```

The committed test ROMs are small and synthetic, so any comparison against commercial titles needs a
local library pointed at by argument or `TETANES_BENCH_ROMS`.

## Configuration

Every knob is an environment variable. `TETANES_BENCH_NO_OUTPUT` and `TETANES_BENCH_NO_AUDIO` are
tested for presence, so any value — including empty — turns them on.

| Variable | Default | Effect |
|---|---|---|
| `TETANES_BENCH_ROMS` | `test_roms/spritecans.nes` | A directory (every `.nes` inside, sorted) or a `:`-separated list of paths. `.nes` arguments on the command line take precedence over it. |
| `TETANES_BENCH_FRAMES` | 600 | Frames clocked per timed iteration. |
| `TETANES_BENCH_ITERS` | 10 | Timed iterations per ROM. The reported statistics are across these. |
| `TETANES_BENCH_WARMUP` | 120 | Untimed frames before each iteration's timing starts. |
| `TETANES_BENCH_NO_OUTPUT` | unset | Time the CPU/PPU/APU core alone, without the `frame_buffer` read that runs `Video::apply_filter`. |
| `TETANES_BENCH_NO_AUDIO` | unset | `HeadlessMode::NO_AUDIO`, and so `Apu::skip_mixing`. |
| `TETANES_BENCH_FILTER` | `ntsc` | `pixellate` selects the cheap palette decode instead of the NTSC filter. Only meaningful without `TETANES_BENCH_NO_OUTPUT`. |
| `TETANES_BENCH_RUN_AHEAD` | 0 | Clock with run-ahead enabled: one console snapshot plus `n` extra frames per call. |

Lowering `FRAMES` and `ITERS` turns the benchmark into a quick smoke sweep over a large ROM library,
where a board that maps its banks wrongly enough to derail the CPU shows up as a frame-clock error.
Boards this emulator does not implement are listed under `SKIPPED` rather than aborting the run.

**`NO_OUTPUT` is the mode to A/B a core change in.** The filter is a roughly constant ~5% offset
that dilutes the delta; leaving it in is what makes the default number honest about a real frame,
not what makes a comparison sharp. `NO_AUDIO` does not skip channel clocking — the channels still
tick and the DMC still steals CPU cycles — so it isolates the cost of turning channel state into
samples, not the cost of the APU.

## What the harness controls for

- **A fresh `ControlDeck` per iteration, not `reset`.** `Bus::reset` clears WRAM but not mapper
  PRG-RAM/SRAM or bank registers, so resetting lets battery saves and bank state carry over and each
  iteration measures a different game state. Super Mario Bros. reports 21.9% cv that way against
  0.17% with a fresh load. The load costs a few ms against ~2 s of timed frames.
- **`RamState::AllZeros`**, so RAM contents are deterministic.
- **Untimed warmup frames**, which settle caches, branch predictors and CPU frequency, and get past
  the ROM's boot sequence.
- **No input is injected**, so commercial ROMs are measured on a title or attract screen rather than
  in gameplay. Stable and repeatable, which is what regression testing needs, but it under-reports a
  busy gameplay frame. `spritecans.nes` is a sprite stress ROM and is the pessimistic end of the
  range.

**The corpus is the point.** Mapper cost varies enormously by board — MMC5 costs ~36% over NROM and
VRC6 ~15%, while on Super Mario Bros. no mapper symbol appears in a profile at all. A single-ROM run
is blind to all of it: an MMC2 board that rebuilt every page on each CHR latch flip cost Punch-Out!!
20% of its frame, and only the seven-ROM corpus saw it.

## Reading the output

A coefficient of variation under ~1% means the run is clean and a 2% change is real. **Treat any ROM
above ~2% cv as an invalid measurement and re-run it** rather than reading its mean. A run taken
under a load average of ~6 reported Punch-Out!! at 21.8% cv and Castlevania III at 13.9% with a max
70% above its true figure.

## Comparing two builds

**A low `cv` within a run does not make two runs comparable.** The same binary measured 2.949 and
2.860 geomean an hour apart — 3.1%, with every ROM under 1.1% cv. The first run of a session is
systematically slow, the first run *after a compile* is ~1.5% slow (eight compiler threads had the
machine moments earlier), and over a long session the whole machine drifts. Taking `before` and
`after` as one run each has produced a confident, entirely fictional 2.5% regression, complete with
a plausible cache-line story.

So:

1. **Build both binaries first**, so no compile overlaps a measurement.
2. **Interleave**: A, B, A, B. Never all of A then all of B.
3. **Discard the first round.**
4. Run nothing else — no test suite, no `cargo check`. Compilation on other cores perturbs a
   `taskset`-pinned run through turbo budget and memory bandwidth.
5. Drop any ROM above ~2% cv for that round.

Anything under ~1% between two interleaved builds is below this machine's noise floor. Do not report
it as a win or a loss.

### The two-layout rule

Basic-block placement alone is worth ±3% here, so a single A/B cannot always tell a real change from
a lucky one. Build each variant twice — once normally, once with
`RUSTFLAGS="-Zthreads=8 -Cllvm-args=-align-all-nofallthru-blocks=5"` — and believe the sign only
when both agree.

**The second layout is owed only to verdicts the perf number itself makes**: a change accepted
*because* it wins, or rejected *despite* being wanted. A fix or refactor kept on its own merits gets
a single default-layout run to rule out a large regression, and a sub-noise-floor null at default
stands without confirmation. The aligned build never ships — it grows `.text` by ~22% and rides on
an unstable interface — which is exactly why it works as an independent second sample of the layout
lottery.

One candidate measured +1.5% at default codegen and -1.5% aligned: a perfectly symmetric draw that
a single run would have read as a regression.

### Profiling gotchas

- **`perf annotate -s <symbol>` reports percentages of that symbol, not of the program.** Since
  `Bus::ppu_clock` is ~45% of the frame, everything it prints has to be scaled before it can sit
  next to a `perf report` share. `--percent-type global-period` does not fix the sorted summary.
- Use `perf report --no-children --sort srcline -e cycles:pp`, whose shares are whole-program
  already, and map each line to its enclosing `fn` by hand — almost the whole PPU inlines into
  `ppu_clock`, so a line range read off a summary can span two inlined functions.
- **When cycles move but instructions, branches and cache misses do not, look at `idq.dsb_uops`
  versus `idq.mite_uops` before looking anywhere else.** A hot function outgrowing the uop cache has
  cost 3% here with an identical instruction stream.

## Findings the source code refers back to

These are the measurements a comment in the tree cites; the rest of the history is in git.

- **Boxing a `Mapper` variant is a measured trade, not a size rule, and it has surprised us in both
  directions.** Un-boxing `Bus::wram` (2 KiB) measured **1.2% slower** despite removing an
  indirection; boxing `SunsoftFme7` (72 bytes) measured **2.2% faster** despite adding one — even on
  the FME7 ROMs themselves, whose audio is clocked every CPU cycle. Neither is predictable from the
  struct size; both are about what else fits in cache alongside.
- **Run-ahead clones the console rather than serializing it**, 1.6x to 5.1x faster per snapshot
  (Castlevania III 0.1715 ms round trip against 0.0333 ms clone). The decode side is what the clone
  avoids: allocating and zeroing the whole `Memory` arena and copying ROM back in. Rewind keeps the
  serialized form and should — it holds ~900 snapshots in RAM at once.
- **`Ppu`'s first cache line is worth defending; placement past it is not worth reasoning about.**
  The per-dot working set fills the first 64 bytes exactly and `tests::print_layouts` asserts it.
  Growing `Mask` by four bytes spilled that line and turned a -2.1% change into +1.0%. Realigning
  `palette` into a single line past that boundary — textbook grounds, read once per visible pixel —
  measured **3.2% slower**.
- **The dot loop is not branch-bound on x86.** Its branches are perfectly predicted, and a perfectly
  predicted branch on an out-of-order core is close to free: removing about four operations per dot
  measured -0.7%, straddling the floor. Stores are a different matter — deleting a derived `fine_y`
  field, and with it a second store per `v` mutation, measured -2.2% and -1.4% across both layouts.
  Candidates argued from saved operations have repeatedly measured null.
- **`get_unchecked` has nothing to buy.** The two bounds-checked hot reads, `Memory::chr_peek` and
  `Frame::set_pixel`, show the check in the disassembly and show it costing at most ~0.5%, only on
  `chr_peek`, and that is the length load rather than the branch. The panic edge is 0.00% in both:
  never taken and laid out cold.

Two build-level notes:

- **Full LTO buys nothing** — `--profile release` measured 3.007 against `--profile perf` 3.014.
  `tetanes-core` is a single crate. The `perf` profile is a faithful stand-in for release, so its
  profiles are representative.
- **`-Ctarget-cpu=x86-64-v3` is worth about 1%** and is not shipped: this workload is byte-at-a-time
  state machine code with nothing to vectorise, and v3 excludes pre-2015 hardware. If it is ever
  measured again, pass it as `RUSTFLAGS` *keeping* `-Zthreads=8` (the env var replaces the workspace
  cargo config's `build.rustflags` wholesale) and read the binary path back from
  `cargo build --message-format=json`, since changing rustflags changes the `-C metadata` hash and
  the old path silently benchmarks the previous build against itself.

## Sample numbers

**These are a sample from one machine at one commit, for orientation. They are not a target and not
a baseline to compare a fresh run against** — an A/B is only meaningful against a build you measured
yourself, in the same session, the same way.

Desktop, 2026-08-01. `--profile perf`, `TETANES_BENCH_NO_OUTPUT=1`, `taskset -c 0`, quiet 16-core
x86-64, 10 x 600 frames with 120 warmup.

| ROM | Mapper | ms/frame |
|---|---|---|
| spritecans | 000 NROM (sprite stress) | 1.716 |
| Super Mario Bros. | 000 NROM | 1.742 |
| Legend of Zelda | 001 MMC1 | 1.774 |
| Super Mario Bros. 3 | 004 MMC3 | 1.959 |
| Punch-Out!! | 009 MMC2 | 1.672 |
| Castlevania III | 005 MMC5 | 2.808 |
| Akumajou Densetsu | 024 VRC6 | 2.323 |
| **geometric mean** | | **1.965** |

Raspberry Pi 5 Model B, 2026-08-03 (Cortex-A76 x4 @ 2.4 GHz, Rocky 10). `--profile perf` built for
`aarch64-unknown-linux-musl` with `rust-lld`, performance governor, `taskset`-pinned, same corpus.

| mode | geomean ms/frame | worst (Castlevania III) |
|---|---|---|
| core only (`TETANES_BENCH_NO_OUTPUT=1`) | 3.275 | 5.20 |
| core + Pixellate (`TETANES_BENCH_FILTER=pixellate`) | 3.371 | 5.23 |
| core + NTSC (default) | 3.451 | 5.31 |

Every mode clears the 16.6 ms budget by at least 3x, NTSC included. Run-to-run variance on the Pi is
0.02-0.34% cv, far tighter than x86, so an ARM-focused session should A/B on target rather than
extrapolate. The one architectural difference worth knowing: **the dot loop is branch-bound there
even though it is not on x86** — IPC 2.26 and L1 miss rates under 0.15%, but a 2.06% branch-miss
rate worth roughly 15% of cycles, spread across the whole loop rather than concentrated in one
convertible select.

For reference against another emulator, measured the same day on the same desktop and corpus with
`--profile release`: TetaNES 1.953 geomean against MesenCE 1.812, a 7.8% gap, worst on MMC3
(+14.8%) and best on VRC6 (+3.3%). **Compare like with like or that number is wrong by half** —
`TETANES_BENCH_NO_OUTPUT=1` is the match for MesenCE's default, which runs its video filter on a
separate thread. Timing the two defaults against each other reads as a ~25% gap because it puts
`Video::apply_filter` on one side and nothing on the other. Both binaries were non-PGO and generic
x86-64, so the comparison is architectural.

Note when reading anything older: `spritecans.nes` is the sole PGO training workload, which biases
branch layout toward mapper 0, and baselines recorded before 2026-07 excluded the output filter, so
they are comparable to `TETANES_BENCH_NO_OUTPUT=1` runs rather than to the current default.
