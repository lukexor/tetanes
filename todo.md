# TODO

- [x] Extract TODO comments
- [ ] Merge GitHub todos
- [ ] Rank and point TODOs

## General

- [x] Verify cpu load on native - add sleeps if necessary (but not for wasm)
- [ ] Run hyperfine on unit tests to benchmark inlining and perf changes
- [ ] Remove long chain of getters/setters - control_deck is the outer boundary
      of the lib
- [ ] Fix fast-forward
- [ ] Create send and recv util methods that logs, handle errors and don't block
- [ ] Experiment with `ControlDeck` being clocked on another thread
- [ ] Move event_loop body to a dedicated function so it can be shared between
      `spawn` and `run`
- [ ] Split up event loop body so redraw/events/etc are in separate functions
- [ ] Only save configs if changed (or only save diffs?)
- [ ] Add CLI flag to ignore saved configs
- [ ] Clone and run winit, pixels, cpa, and egui examples
- [ ] Fix frame timing/cpu usage

## Audio

- [ ] Rename mixer to `Audio` and `Playback` to `Output`
- [ ] Verify mono channels comes out both speakers
- [ ] Verify filters with visualizations/unit tests
- [x] Explore circular buffer that overwrites oldest samples when full Try
      inserting all 29k samples per frame and processing them in audio callback
  - [x] Chunk audio samples when pushing for audio callback
- [ ] Show error and disable audio if no valid device/config can be found
- [ ] Add `rubato` crate for down-sampling
- [ ] Allow selecting output device from config menu
- [ ] Experiment with an 8 \* 512 buffer size (8 blocks of 512, e.g. audio stream
      is set to 512 but internally 8 blocks are kept)
- [ ] Experiment clocking partial frames (buffer size worth) from audio thread
- [ ] Debug visualizations of pulse, triangle, sawtooth, noise, and dcm channels
      during play as well as combined waveform
- [ ] tests/apu.rs APU integration tests
- [ ] Add saving `.wav` format for audio recording
- [ ] Create shared circular buffer of Vecs to avoid allocations
- [ ] Experiment requesting redraw from audio thread

## WASM

- [ ] Explore web workers for control_deck clocking
- [ ] Explore audio worklets
- [ ] Ensure no blocking operations in wasm code paths
- [ ] Figure out how to give time back on wasm instead of looping?
- [ ] Add lib methods that take wasm output and performs what run-wasm does
  - [ ] Add bin method that utilizies lib same as run-wasm
  - [ ] Utilize lib method inside lukeworks to build TetaNES Web page
- [ ] Add filesystem trait for abstracting rom/config/save state storage
- [ ] Run `twiggy` to analyze wasm bundle size
- [ ] Switch event loop to use `event.spawn` instead of `event.run` - the latter
      uses exceptions for control flow, not great
- [ ] Add trait for abstracting render work/threading/web workers/etc
- [ ] When adding click event listeners, do it on the body and use the click
      target to switch on which button was clicked - one event listener instead
      of many
- [ ] Allow drag/drop file for loading ROMs
- [ ] Focus canvas when loading a ROM

## UI

- [ ] Add egui
- [ ] Pause/resume when window is moved to avoid stutter
- [ ] Add `strum` crate for menus and enum iteration/stringifing of options
- [ ] Add `MessageType` enum for displaying Info, Debug, Warn, and Error
      messages on the screen.
- [ ] Support window resizing, pause emulation while doing so
- [ ] Add confirm exit configuration and handle in `CloseRequested` event
- [ ] Fix window icon on macos - winit lacks support
- [ ] Fix toggling Vsync - needs to create pixels instance
- [ ] Fix drawing zapper crosshair
- [ ] Create shared circular buffer of Vecs to avoid allocations
- [ ] Restore default `save_on_exit` configuration when done debugging
- [ ] Add fullscreen mode configuration - Windowed Borderless/Fullscreen
- [ ] Add always on top configuration
- [ ] Update available video modes and window scale when window moved for fullscreen

## Input

- [ ] Add shortcut for `<C-+>` and `<C-->` to change window scale
- [ ] Rename events module to keybinds?
- [ ] Update bindings to use LogicalKey instead of PhysicalKey and only trigger
      on key-release instead of key-press
- [ ] Add debug keybindings for incrementing/decrementing buffer size and audio delay
- [ ] Fix controller support

## Debugging

- [ ] Add FPS to screen using simple text function until EGUI is done
- [ ] Add thread name to threads
- [ ] Add custom profiling tracking/visualizations similar to puffin
- [ ] Add CPU debugger
- [ ] Add PPU debugger
- [ ] Add APU debugger

## CPU

- [ ] Refactor fetch code to reduce branches
- [ ] scanline length/nmi timing
- [ ] roms/super_mario_bros_3.nes
- [ ] roms/fire_hawk.nes
- [ ] roms/laser_invasion.nes
- [ ] test_roms/cpu/overclock.nes

## PPU

- [ ] Ppu::tick() very hot - optimize

## Docs

- [ ] Review all methods, internal and external for learning documentation with
      links and references
