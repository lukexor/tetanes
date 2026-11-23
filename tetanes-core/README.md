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
  NES-game into a single executable targeting your favorite OS!
- [Dappicom](https://github.com/tonk-gg/dappicom) - Dappicom is a provable
  Nintendo Entertainment System emulator written in Noir and Rust.
- [NESBox](https://github.com/mantou132/nesbox/) - NESBox's vision is to become
  the preferred platform for people playing online multiplayer games, providing
  an excellent user experience for all its users.

## Minimum Supported Rust Version (MSRV)

The current minimum Rust version is `1.88.0`.

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
        // A display frame is however many NES frames the emulation speed asks for, so clock
        // until it reports the display frame is done. At the default 1x that is one pass.
        while control_deck.clock_frame()? == Clocked::Continue {}

        // The audio this frame produced. Queue it to an audio device. Each clock
        // discards the previous call's samples, so nothing accumulates - see
        // `Config::clear_audio_on_clock` if you would rather drain them yourself.
        let audio_samples = control_deck.audio_samples();

        // The frame to display, as RGBA pixels. Blit it to the screen.
        // See also: `ControlDeck::frame_buffer_into` to render into a buffer you
        // already own, and `frame_buffer_raw` for undecoded PPU pixels.
        let frame_buffer = control_deck.frame_buffer();

        // If not relying on vsync, sleep or otherwise wait the remainder of the
        // 16ms frame time to clock again.
    }

    Ok(())
}
```

To hide input lag, have the deck run ahead of itself and display a frame from the
future, rewinding back each time:

```rust no_run
# use tetanes_core::prelude::*;
# let mut control_deck = ControlDeck::new();
control_deck.set_run_ahead(2);
```

Input goes through `ControlDeck::joypad_mut`, and `ControlDeck::save_state` /
`load_state` capture and restore the whole console.

## Stability

`tetanes-core` follows semantic versioning. The stable API is everything public
with default features: `ControlDeck`, its `Config`, and the types its methods
take and return.

`ControlDeck`'s fields are private, and its accessors are not pass-throughs.
`frame_buffer` applies the video filter, cached behind a dirty flag.
`frame_buffer_raw` picks the run-ahead (older) frame over the most recent one.
`sram` is a no-op unless the cart has a battery.

### The `unstable` feature

The `unstable` feature makes the emulated components public. It also adds the
`ControlDeck` methods that reach them, such as `bus`, `bus_mut`, `load_bus` and
`clock_frame_until`, and the debugger hooks. A debugger, a cheat engine or a
machine-learning harness can then read and write the whole machine.

Semantic versioning does not cover these items, and any release may add,
rename or retype them. Pin the exact version when you enable the feature:

```toml
tetanes-core = { version = "=1.0.0", features = ["unstable"] }
```

A mix of `pub` fields and getters is deliberate here:

- `Bus`, `Cpu`, `Ppu`, `Apu`, `Input` and every mapper board expose their
  fields as `pub`. Getters exist where a field is wrapped in a container.
  `Bus::wram` returns `&[u8; 2048]` from a `Box<ConstArray<..>>`, and
  `Ppu::frame_buffer` dereferences a `Frame`.
- `Memory`, `Cart`, `PaletteRam` and `Scroll` have private fields, because
  their getters apply extra logic. `Memory::sram` splices PRG-RAM together with
  the used prefix of the battery region. `Cart::mapper_num` prefers the game
  database over the header. `Cart::prg_rom` trims the padding `Memory` added to
  reach a whole page. `PaletteRam` applies `$3F10`/`$14`/`$18`/`$1C` mirroring
  on every access.

`Ppu`'s render-gating caches and `Bus::mapper_ops` are `pub`, with a comment
warning that writing them desynchronizes the emulator. `Memory`'s page tables
are private behind read-only getters, because `data`, `ram_start` and the
region ranges have to agree with each other. The PPU's caches are scalars
recomputed from a register write.

## Known Issues

See the [github issue tracker][].

### Contact

For issue reporting, please use the [github issue tracker][]. You can also
contact me directly at <https://lukeworks.tech/contact/>.

[github issue tracker]: https://github.com/lukexor/tetanes/issues
