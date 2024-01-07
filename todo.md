# TODO

- [x] Extract TODO comments
- [x] Merge GitHub todos
- [ ] archive Github project todos
- [ ] Rank and point TODOs
- [ ] Clone and run winit, pixels, cpa, and egui examples

## Notes

- Ensure spawn closures use an extra block scope to clone values to retain
  variable names

## General

- [ ] Fix recursively dropped wasm error and perf/audio quality
- [ ] Try integrating bytes crate for frames or samples
- [ ] Create shared circular buffer of Vecs to avoid allocations
- [ ] Add back `ringbuf` crate and use `pop_slice` and `push_slice`
- [ ] Fix fast-forward
- [ ] Track all video/audio timing and try to visualize graph of torn/dropped
      frames and audio under/overruns
- [ ] Only save configs if changed (or only save diffs?)
- [ ] Remove long chain of getters/setters - control_deck is the outer boundary
      of the lib
- [ ] Create send and recv util methods that logs, handle errors and don't block
- [ ] Experiment with `ControlDeck` being clocked on another thread
- [ ] Fix toggling Vsync - needs to create pixels instance
- [ ] Auto save
- [x] Verify cpu load on native - add sleeps if necessary (but not for wasm)
- [x] Fix frame timing/cpu usage
- [x] Move `event_loop` body to a dedicated function so it can be shared between
      `spawn` and `run`
- [x] Split up event loop body so redraw/events/etc are in separate functions
- [x] Handle when `vsync` is enabled and `Mailbox` is not - hard to support
      falling back if Mailbox isn't available, not that important

## Audio

- [ ] Verify filters with visualizations/unit tests
- [ ] Show error and disable audio if no valid device/config can be found
- [ ] Add `rubato` crate for down-sampling
- [ ] Allow selecting output device from config menu
- [ ] Experiment clocking partial frames (buffer size worth) from audio thread
- [ ] Debug visualizations of pulse, triangle, sawtooth, noise, and dcm channels
      during play as well as combined waveform
- [ ] tests/apu.rs APU integration tests
- [ ] Add saving `.wav` format for audio recording. Use `hound` crate
- [ ] Create shared circular buffer of Vecs to avoid allocations
- [ ] Pause when title receives mouse down
      <https://github.com/rust-windowing/winit/issues/1885>
- [ ] Add debug keybindings for incrementing/decrementing buffer size and audio delay
- [ ] Fix audio latency to match expectation
- [ ] Add `make_stream` fn generic over sample type, remove `Callback` struct
- [x] Explore circular buffer that overwrites oldest samples when full Try
      inserting all 29k samples per frame and processing them in audio callback
  - [x] Chunk audio samples when pushing for audio callback
- [x] Experiment with an 8 \* 512 buffer size (8 blocks of 512, e.g. audio stream
      is set to 512 but internally 8 blocks are kept)
- [x] Experiment requesting redraw from audio thread

## WASM

- Ensure no blocking operations in wasm code paths

- [ ] Add trait for abstracting render work/threading/web workers/etc
- [ ] Explore web workers for control_deck clocking
- [ ] Explore audio worklets
- [ ] Experiment with `wasm-threads` crate
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

## Input

- [ ] Fix controller support
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
- [ ] More mappers
- [ ] wideNES feature
- [ ] Network multiplay
- [ ] Light mode/dark mode

## Release test plan

- [ ] Macos/linux/windows binary releases
- [ ] Run `twiggy` to analyze wasm bundle size
- [ ] unit tests (duh)
- [ ] Test several roms per mapper
- [ ] Ensure CPU usage is low while playing and in BG
- [ ] Test 2, 3 and 4 player
- [ ] Verify wasm performance
- [ ] Save/load states
- [ ] Save/load sram
- [ ] Controller for 1 and 2 players
- [ ] Zapper
- [ ] Rewind
- [ ] Record/playback
- [ ] Audio recording
- [ ] non cycle-accurate
- [ ] Toggle audio channels
