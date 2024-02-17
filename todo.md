# TODO

## Project

- [ ] Rank and point TODOs
- [ ] Clone and run winit, pixels, cpa, and egui examples - draft task ideas
- [ ] archive Github project todos
- [ ] Add todo autocommands/shortcuts
  - Enter on a todo line creates a new todo
  - shortcut to toggle completion
  - shortcut to strikethrough
  - shortcut to toggle bullet to/from todo

## High-level Priorities

- [ ] fix nmi/irq timing - 62aefa85ea810a41209ebc5fc7e541b34e573f6b
- [ ] performance
- [ ] wasm
- [ ] audio dsp

## API Design Reference

- [ ] ref: compile api design cheatsheet
- Ensure spawn closures use an extra block scope to clone values to retain
  variable names
- Ensure large generic functions use a private concrete function to reduce
  monomorphization bloat
- `get(s: &Self) -> T` and `get(s: &mut Self) -> T` instead of `get(&self)` or
  `get(&mut self)` if that Self implements `Deref`/`DerefMut`
- if/when using `thread::park` - ensure it's used in a loop to avoid spurious wakeups

## Performance Tuning

- [ ] Fix puffin_egui
- [ ] Add string_pool for sending messages or just add messages and error later
- [ ] Run cachegrind - Maybe build an internal Cpu/Ppu cache
- [ ] Perform perf tests:
  - [ ] Test single vs multi-threaded
  - [ ] CPU load playing, paused, occluded
  - [ ] Profile Vsync, No Vsync
  - [ ] Test wasm
- [ ] Improve cache locality/function size - remove branches in hot loops
- [ ] analyze opportunities for `rayon`
- [ ] Add custom profiling tracking/visualizations similar to puffin
- [ ] Track all video/audio timing and try to visualize graph of torn/dropped
      frames and audio under/overruns
- [ ] Experiment: jemalloc
- [ ] use fetch add atomic for generating unique IDs for profiling

  ```rust
  NEXT_ID.fetch_update(Relaxed, Relaxed, |n| n.checked_add(1)).expect("too many IDs!")
  ```

- [x] Experiment: `ControlDeck` being clocked on another thread
- [x] reduce cpu usage in main thread
- [x] ~~Add back `ringbuf` crate and use `pop_slice` and `push_slice` - switched
      to ThingBuf instead.~~
- [x] ~~Try integrating bytes crate for frames or samples - not sure if really
      needed at this point with ThingBuf~~
- [x] Create shared circular buffer of Vecs to avoid allocations
- [x] Blackbox benchmark Cpu/Ppu to tweak performance changes
- [x] Remove `inline` from non-trivial functions, break up generics with
      concrete impls

## Error Handling

- [ ] add thiserror for recoverable and non-recoverable errors
- [ ] Disable control deck running on cpu corrupted, require reset

## Renderer

- [ ] Fix top/bottom trim on NTSC 8 lines with menu bar
- [ ] Fix toggling Vsync - needs to re-create pixels instance
- [x] ~~Provide way for Video decode to write directly to buffer to avoid extra
      copying~~

## New Features

- [ ] Auto save every X seconds (configurable) slot 0
- [ ] Add saving `.wav` format for audio recording. Use `hound` crate

## UI

- [ ] fix screensaver starting while running
- [ ] Add emojis to menus
- [ ] Add save slot to title bar
- [ ] Add hide/show menu bar & shortcut
- [ ] Add key bindings to UI menu labels
- [ ] Pause emulation while moving/resizing
- [ ] Keybindings menu
- [ ] Recent game selection
- [ ] Add `strum` crate for menus and enum iteration/stringifing of options
- [ ] Add confirm exit configuration and handle in `CloseRequested` event
- [ ] Fix drawing zapper crosshair
- [ ] Add fullscreen mode configuration - Windowed Borderless/Fullscreen
- [ ] Update available video modes and window scale when window moved for fullscreen
- [ ] Add game genie menu with save
- [ ] Toggle FPS
- [ ] Toggle messages
- [ ] Add UI button to reset saved config to default
- [ ] Config path overrides
- [ ] NTSC overscan toggle
- [ ] Add always on top configuration
- [ ] Fix window icon on macos - winit lacks support
- [ ] Toggle MMC3 IRQ setting
- [x] Add clear savestate buttons
- [x] ~~Add `MessageType` enum for displaying Info, Debug, Warn, and Error
      messages on the screen.~~
- [x] fix monitor dpi resize when moving across monitors
- [x] Add egui
- [x] Allow drag/drop file for loading ROMs
- [x] Pause/resume when window is moved to avoid stutter

## Audio

- [ ] Add debug keybindings for incrementing/decrementing buffer size and audio latency
- [ ] Pause when title receives mouse down
      <https://github.com/rust-windowing/winit/issues/1885> (I think this is fixed
      by filling with 0s)
- [ ] Show error and disable audio if no valid device/config can be found
  - requires update loop that doesn't depend on audio
- [ ] Update Mixer `play` to return available sample rates to constrain `Config`
- [ ] Compare audio graph with/without filters
- [ ] Add `rubato` crate for down-sampling
- [ ] Update filters to be in frequency domain - fft, filter, reverse fft
- [ ] Verify filters with visualizations/unit tests
- [x] Create shared circular buffer of Vecs to avoid allocations - ThingBuf
- [x] Fix audio latency to match expectation
- [x] ~~Experiment: clocking partial frames (buffer size worth) from audio thread~~
- [x] Prefer desired sample rate as f32
- [x] Add `make_stream` fn generic over sample type, remove `Callback` struct
- [x] Explore circular buffer that overwrites oldest samples when full Try
      inserting all 29k samples per frame and processing them in audio callback
  - [x] Chunk audio samples when pushing for audio callback
- [x] Experiment with an 8 \* 512 buffer size (8 blocks of 512, e.g. audio stream
      is set to 512 but internally 8 blocks are kept)
- [x] Experiment requesting redraw from audio thread

## WASM

- Ensure no blocking operations in wasm code paths

- [ ] Get basic WASM framerate/audio timing working
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
- [ ] When adding click event listeners, do it on the body and use the click
      target to switch on which button was clicked - one event listener instead
      of many
- [x] Focus canvas when loading a ROM
- [x] Switch event loop to use `event.spawn` instead of `event.run` - the latter
      uses exceptions for control flow, not great

## General

- [ ] research: Explore more traits/new type wrappers to break up functionality
      in `nes` modules
- [ ] Maybe split tetanes/nes into separate crates to decrease dev compile times
  - find a similar project with ui/backend to compare
- [x] fix zapper gun
- [x] ensure trace logs are compiled out of hot paths
- [x] Remove clocking msg from main thread, switch to control flow wait
- [x] Rename Backend to Threads
- [x] Add `Debug` impl for Event to not log `LoadRom`
- [x] Move control deck state storage to control deck module
- [x] Rename buffer_pool to frame_pool
- [x] change handle_error to map handle_result
- [x] add puffin_egui
- [x] Fix rad racer ii
- [x] ~~Add Storage trait to reduce memory for saves/rewinds/replays
      (After review, not necessary as serialize does it pretty well and I can
      skip any fields not required for restoring state)~~
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

## CI

- [ ] Change CI to just check and test
- [ ] Remove -D warnings from CI and just keep for PRs

## Configuration

- [ ] only save diffs
- [ ] Change save dir on Windows to AppData
- [ ] Allow selecting audio output device

## Input

- [ ] Fix controller support for 4 players
- [ ] Add shortcut for `<C-+>` and `<C-->` to change window scale

## Debugging

- [ ] Review all debug impls for accuracy
- [ ] Add CPU debugger
  - [ ] instr/memory read/write/exec breakpoints
  - [ ] Modify registers/memory
  - [ ] enable Label regions/addresses
  - [ ] Show irq/nmi status
  - [ ] Modify input state for 4 controllers
  - [ ] step in, out, over, scanline, frame
- [ ] Add PPU debugger
  - [ ] Nametable/Chr/Sprite/OAM views
  - [ ] Palette viewer
  - [ ] Palette selection
  - [ ] Modify registers/memory
  - [ ] Toggle title grid lines
  - [ ] Scanline/cycle sliders
  - [ ] Tile select with preview
  - [ ] Last known palette
- [ ] Add APU debugger
  - [ ] Modify registers
  - [ ] Enable/disable channels
  - [ ] plot visuals per channel and combined
- [ ] Update debug impls with hex values
- [ ] Switch to tracing crate w/ file logging
- [x] Performance bench
- [x] Run hyperfine on unit tests to benchmark inlining and perf changes
- [x] Add thread name to threads

## CPU

- [ ] Review Scanline length/nmi timing
- [ ] roms/laser_invasion.nes
- [x] roms/super_mario_bros_3.nes
- [x] roms/fire_hawk.nes
- [x] Refactor fetch code to reduce branches

## PPU

- [ ] Ppu::tick() very hot - optimize

## Tests

- [x] test_roms/cpu/overclock.nes
- [ ] fix failing PPU rom tests
- [ ] fix failing APU rom tests

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
- [ ] Add `pix-gui` crate for rendering text and rects to start
- [ ] Draw profiling metrics via rects for last 60 frames with 30ms scale for window
      width
