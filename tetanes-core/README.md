<!-- markdownlint-disable no-inline-html no-duplicate-heading -->

# TetaNES Core

[![Build Status]][build] [![Doc Status]][docs] [![Latest Version]][crates.io]
[![Downloads]][crates.io] [![License]][gnu]

[build status]: https://img.shields.io/github/actions/workflow/status/lukexor/tetanes/ci.yml?branch=main
[build]: https://github.com/lukexor/tetanes/actions/workflows/ci.yml
[doc status]: https://img.shields.io/docsrs/tetanes-core?style=plastic
[docs]: https://docs.rs/tetanes-core/
[latest version]: https://img.shields.io/crates/v/tetanes-core?style=plastic
[crates.io]: https://crates.io/crates/tetanes-core
[downloads]: https://img.shields.io/crates/d/tetanes-core?style=plastic
[license]: https://img.shields.io/crates/l/tetanes-core?style=plastic
[gnu]: https://github.com/lukexor/tetanes/blob/main/LICENSE-MIT

<!-- markdownlint-disable line-length -->
📖 [Summary](#summary) - ✨ [Features](#features) - 🚧 [Building](#building) - 🚀 [Getting
Started](#getting-started) - ⚠️ [Known Issues](#known-issues) - 💬 [Contact](#contact)
<!-- markdownlint-enable line-length -->

## Summary

<img width="100%" alt="TetaNES"
  src="https://raw.githubusercontent.com/lukexor/tetanes/main/static/tetanes.png">

> photo credit for background: [Zsolt Palatinus](https://unsplash.com/@sunitalap)
> on [unsplash](https://unsplash.com/photos/pEK3AbP8wa4)

This is the core emulation library for `TetaNES`. Savvy developers can build their
own custom emulation libraries or applications in Rust on top of `tetanes-core`.

Some community examples:

- [NES Bundler](https://github.com/tedsteen/nes-bundler) - Transform your
  NES-game into a single executable targeting your favourite OS!
- [Dappicom](https://github.com/tonk-gg/dappicom) - Dappicom is a provable
  Nintendo Entertainment System emulator written in Noir and Rust.
- [NESBox](https://github.com/mantou132/nesbox/) - NESBox's vision is to become
  the preferred platform for people playing online multiplayer games, providing
  an excellent user experience for all its users.

## Minimum Supported Rust Version (MSRV)

The current minimum Rust version is `1.78.0`.

## Features

- NTSC, PAL and Dendy emulation.
- Headless Mode.
- Pixellate and NTSC filters.
- Zapper (Light Gun) support.
- iNES and NES 2.0 ROM header formats supported.
- Over 30 supported mappers covering >90% of licensed games.
- Game Genie Codes.
- Preference snd keybonding menus using [egui](https://egui.rs).
  - Increase/Decrease speed & Fast Forward
  - Save & Load States
  - Battery-backed RAM saves

### Building

To build the project, you'll need a nightly version of the compiler and run
`cargo build` or `cargo build --release` (if you want better framerates).

#### Feature Flags

- **profiling** - Enables [puffin](https://github.com/EmbarkStudios/puffin)
  profiling.

### Getting Started

Below is a basic example of setting up `tetanes_core` with a ROM and running the
emulation. For a more in-depth example see the `tetanes::nes::emulation` module.

```rust no_run
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

## Known Issues

See the [github issue tracker][].

### Contact

For issue reporting, please use the [github issue tracker][]. You can also
contact me directly at <https://lukeworks.tech/contact/>.

[github issue tracker]: https://github.com/lukexor/tetanes/issues
