# TetaNES Core

## Summary

This is the core emulation library for TetaNES. Savvy developers can build their
own libraries or UIs on top of it.

## Minimum Supported Rust Version (MSRV)

The current minimum Rust version is `1.78.0`.

## Feature Flags

- **cycle-accurate** - Enables cycle-accurate emulation. More CPU intensive, but
  supports a wider range of games requiring precise timing. Disabling may
  improve performance on lower-end machines. Enabled by default.
- **profiling** - Enables [puffin](https://github.com/EmbarkStudios/puffin)
  profiling.

### Building

To build the project, you'll need a nightly version of the compiler and run
`cargo build` or `cargo build --release` (if you want better framerates).

### Getting Started

Below is a basic example of setting up `tetanes_core` with a ROM and running the
emulation. For a more in-depth example see the `tetanes::nes::emulation` module.

```rust
use tetanes_core::prelude::*;

fn main() -> anyhow::Result<()> {
    let mut control_deck = ControlDeck::new();

    // Load a ROM from the filesystem.
    // See also: `ControlDeck::load_rom` for loading anything that implements `Read`.
    control_deck.load_rom_path("some_awesome_game.nes")?;

    while control_deck.is_running() {
      // See also: `ControlDeck::clock_frame_output` and `ControlDeck::clock_frame_into`
      control_deck.clock_frame()?;

      let audio_samples = control_deck.audio_samples();
      // Process audio samples (e.g. by sending it to an audio device)
      control_deck.clear_audio_samples();

      let frame_buffer = control_deck.frame_buffer();
      // Process frame buffer (e.g. by rendering it to the screen)

      // If not relying on vsync, sleep or otherwise wait the remainder of the
      // 16ms frame time to clock again
    }

    Ok(())
}
```
