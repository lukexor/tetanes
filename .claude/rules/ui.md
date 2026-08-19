---
paths:
  - "tetanes/src/**/*"
  - "tetanes-core/src/sys/**/*"
---

# UI crate and platform abstraction

The `tetanes` binary: winit event loop, egui GUI, wgpu renderer, and the `cfg`-selected `sys::`
layer both crates share.

## Structure

`Nes` (`nes.rs`) is the winit `ApplicationHandler`, holding a `State` machine
(`Suspended`, `Pending`, `Running`, `Exiting`) because wgpu/window resources are created
asynchronously. `Running` owns:

- **`Emulation`** (`nes/emulation.rs`) runs the `ControlDeck`. It has two backends: `Threads::Multi`
  (emulation on its own thread, self-clocking, woken via `unpark`) and `Threads::Single` (clocked
  from the event loop). Selection is `cfg.emulation.threaded` (CLI `--no-threaded`) AND
  `available_parallelism() > 1`, so single-threaded runs on wasm. Frames reach the renderer over a
  `thingbuf` channel with a `FrameRecycle` pool to avoid per-frame allocation.
- **`Renderer`** (`nes/renderer.rs`) is egui plus wgpu, multi-viewport aware, with the emulator
  frame drawn through a custom `painter`/`shader`/`texture` path and the GUI in `renderer/gui.rs`.

All communication is via `NesEvent` (`nes/event.rs`) pushed through a winit `EventLoopProxy`
(`NesEventProxy`), split into `EmulationEvent`, `RendererEvent`, `ConfigEvent`, `DebugEvent`, and
`UiEvent`. Adding a feature that crosses the emulation/UI boundary means adding a variant there
rather than sharing state.

## Platform abstraction

Both crates use the same pattern: a public façade module that `pub use`s a `sys::` implementation
selected by `cfg`, with parallel `os.rs` / `wasm.rs` files
(`tetanes/src/sys/{platform,logging,thread,info}/`, `tetanes-core/src/sys/{fs,time}/`). Capability
checks at runtime go through `platform::Feature` (`Filesystem`, `Storage`, `Suspend`,
`ScreenReader`, and so on) rather than raw `cfg` in UI code. Anything touching files, threads, time,
or clipboard needs both sides implemented, and the wasm clippy/doc CI jobs catch omissions.
