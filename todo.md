# TODO

## High-level Priorities

- [ ] Get basic WASM framerate/audio timing working
- [ ] Add puffin-like profiling metrics
- [ ] Add `pix-gui` crate for rendering text and rects to tart
- [ ] Draw profiling metrics via rects for last 60 frames with 30ms scale for window
      width
- [ ] Render last frame duration text

## Notes

- Ensure spawn closures use an extra block scope to clone values to retain
  variable names

## API Design Reference

- `get(s: &Self) -> T` and `get(s: &mut Self) -> T` instead of `get(&self)` or
  `get(&mut self)` if that Self implements `Deref`/`DerefMut`
- if/when using `thread::park` - ensure it's used in a loop to avoid spurious wakeups

## General

- [x] Fix rad racer ii
- [ ] archive Github project todos
- [ ] Rank and point TODOs
- [ ] Clone and run winit, pixels, cpa, and egui examples
- [ ] ref: compile api design cheatsheet
- [ ] use fetch add atomic for generating unique IDs for profiling

  ```rust
  NEXT_ID.fetch_update(Relaxed, Relaxed, |n| n.checked_add(1)).expect("too many IDs!")
  ```

- [ ] research: Explore more traits/new type wrappers to break up functionality
      in `nes` modules
- [-] Add Storage trait to reduce memory for saves/rewinds/replays
  (After review, not necessary as serialize does it pretty well and I can
  skip any fields not required for restoring state)
- [x] Remove long chain of getters/setters - control_deck is the outer boundary
      of the lib
- [x] Extract TODO comments
- [x] Merge GitHub todos
- [x] Fix fast-forward
- [x] Verify cpu load on native - add sleeps if necessary (but not for wasm)
- [x] Fix frame timing/cpu usage
- [x] Move `event_loop` body to a dedicated function so it can be shared between
      `spawn` and `run`
- [x] Split up event loop body so redraw/events/etc are in separate functions
- [x] Handle when `vsync` is enabled and `Mailbox` is not - hard to support
      falling back if Mailbox isn't available, not that important

## Performance Tuning

- [ ] Run cachegrind on Linux - Maybe build an internal Cpu/Ppu cache
- [x] Blackbox benchmark Cpu/Ppu to tweak performance changes
- [ ] Improve cache locality/function size - remove branches in hot loops
- [ ] Try integrating bytes crate for frames or samples
- [ ] Create shared circular buffer of Vecs to avoid allocations
- [ ] Add back `ringbuf` crate and use `pop_slice` and `push_slice`
- [ ] Track all video/audio timing and try to visualize graph of torn/dropped
      frames and audio under/overruns
- [ ] Experiment: jemalloc
- [ ] Experiment: `ControlDeck` being clocked on another thread
- [x] Remove `inline` from non-trivial functions, break up generics with
      concrete impls

## Renderer

- [ ] Provide way for Video decode to write directly to buffer to avoid extra
      copying
- [ ] Fix toggling Vsync - needs to re-create pixels instance

## Audio

- [ ] Prefer desired sample rate as f32
- [ ] Update Mixer `play` to return available sample rates to constrain `Config`
- [ ] Update filters to be in frequency domain - fft, filter, reverse fft
- [ ] Verify filters with visualizations/unit tests
- [ ] Show error and disable audio if no valid device/config can be found
- [ ] Add `rubato` crate for down-sampling
- [ ] Allow selecting output device from config menu
- [ ] Experiment: clocking partial frames (buffer size worth) from audio thread
- [ ] Debug visualizations of pulse, triangle, sawtooth, noise, and dcm channels
      during play as well as combined waveform
- [ ] tests/apu.rs APU integration tests
- [ ] Add saving `.wav` format for audio recording. Use `hound` crate
- [ ] Create shared circular buffer of Vecs to avoid allocations
- [ ] Pause when title receives mouse down
      <https://github.com/rust-windowing/winit/issues/1885>
- [ ] Add debug keybindings for incrementing/decrementing buffer size and audio delay
- [ ] Fix audio latency to match expectation
- [x] Add `make_stream` fn generic over sample type, remove `Callback` struct
- [x] Explore circular buffer that overwrites oldest samples when full Try
      inserting all 29k samples per frame and processing them in audio callback
  - [x] Chunk audio samples when pushing for audio callback
- [x] Experiment with an 8 \* 512 buffer size (8 blocks of 512, e.g. audio stream
      is set to 512 but internally 8 blocks are kept)
- [x] Experiment requesting redraw from audio thread

## New Features

- [ ] Auto save every X seconds (configurable)

## WASM

- Ensure no blocking operations in wasm code paths

- [ ] Fix recursively dropped wasm error and perf/audio quality
- [ ] Add trait for abstracting render work/threading/web workers/etc
- [ ] Experiment: web workers for control_deck clocking
- [ ] Experiment: audio worklets
- [ ] Experiment: with `wasm-threads` crate
- [ ] Figure out how to give time back on wasm instead of looping?
- [ ] Add lib methods that take wasm output and performs what run-wasm does
  - [ ] Add bin method that utilizies lib same as run-wasm
  - [ ] Utilize lib method inside lukeworks to build TetaNES Web page
- [ ] Add filesystem trait for abstracting rom/config/save state storage
- [ ] Focus canvas when loading a ROM
- [ ] When adding click event listeners, do it on the body and use the click
      target to switch on which button was clicked - one event listener instead
      of many
- [ ] Allow drag/drop file for loading ROMs
- [x] Switch event loop to use `event.spawn` instead of `event.run` - the latter
      uses exceptions for control flow, not great

## CI

- [ ] Change CI to just check and test
- [ ] Remove -D warnings from CI and just keep for PRs

## UI

- [ ] fix monitor dpi resize when moving across monitors
- [ ] Add egui
- [ ] Fix window icon on macos - winit lacks support
- [ ] Support window resizing, pause emulation while doing so
- [ ] Keybindings menu
- [ ] Recent game selection
- [ ] Add `strum` crate for menus and enum iteration/stringifing of options
- [ ] Add `MessageType` enum for displaying Info, Debug, Warn, and Error
      messages on the screen.
- [ ] Add confirm exit configuration and handle in `CloseRequested` event
- [ ] Fix drawing zapper crosshair
- [ ] Add fullscreen mode configuration - Windowed Borderless/Fullscreen
- [ ] Update available video modes and window scale when window moved for fullscreen
- [ ] Add game genie menu with save
- [ ] Add clear savestate buttons
- [ ] Toggle FPS
- [ ] Toggle messages
- [ ] Add UI button to reset saved config to default
- [ ] Config path overrides
- [ ] NTSC overscan toggle
- [ ] Add always on top configuration
- [ ] Toggle MMC3 IRQ setting
- [x] Pause/resume when window is moved to avoid stutter

## Configuration

- [ ] Only save if changed (or only save diffs?)
- [ ] Change save dir on Windows to AppData

## Input

- [ ] Fix controller support for 4 players
- [ ] Add shortcut for `<C-+>` and `<C-->` to change window scale
- [ ] Rename events module to keybinds?

## Debugging

- [ ] Add FPS to screen using simple text function until EGUI is done
- [ ] Add custom profiling tracking/visualizations similar to puffin
- [ ] Add CPU debugger
  - [ ] instr/memory read/write breakpoints
  - [ ] Modify registers/memory
  - [ ] enable Label regions/addresses
  - [ ] Show irq/nmi status
  - [ ] Modify input state for 4 controllers
  - [ ] step in, out, over, scanline, frame
- [ ] Add PPU debugger
  - [ ] Nametable/Chr/Sprite/OAM views
  - [ ] Palette viewer
  - [ ] Palette select
  - [ ] Modify registers
  - [ ] Toggle grid lines
  - [ ] scanline/cycle slider
  - [ ] Tile select with preview
  - [ ] last known palette
- [ ] Add APU debugger
- [ ] Performance benchmark suite
- [ ] Run hyperfine on unit tests to benchmark inlining and perf changes
- [ ] Update debug impls with hex values
- [ ] Add file logging
- [x] Add thread name to threads

## CPU

- [ ] Refactor fetch code to reduce branches
- [ ] Scanline length/nmi timing
- [ ] roms/super_mario_bros_3.nes
- [ ] roms/fire_hawk.nes
- [ ] roms/laser_invasion.nes
- [ ] test_roms/cpu/overclock.nes

## PPU

- [ ] Ppu::tick() very hot - optimize
- [ ] fix failing rom tests

## APU

- [ ] fix failing rom tests

## Docs

- [ ] Update readme
- [ ] Review all methods, internal and external for learning documentation with
      links and references

## Someday features

- [ ] Self update installer
- [ ] Headless mode
- [ ] CRT and other filters (via shaders)
- [ ] Complete NES 2.0 support
- [ ] More mappers
- [ ] wideNES feature
- [ ] Network
