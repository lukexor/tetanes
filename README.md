# RustyNES

## Summary

`RustyNES` is an emulator for the Nintendo Entertainment System (NES) released in 1983, written
using Rust and SDL2.

It started as a personal curiosity project that turned into a project for two classes to demonstrate
a proficiency in Rust and in digital sound production. It is still a work-in-progress, but I hope to
continue development on it and complete it to satisfactory level to play most games. It's my hope to
see a Rust emulator rise in popularity and compete with the popular C and C++ versions.

`RustyNES` is also meant to showcase how clean and readable low-level Rust programs can be in addition
to them having the type and memory-safety guarantees that Rust is known for.

## Screenshots

<img src="https://github.com/lukexor/rustynes/blob/master/static/donky_kong_title.png" width="300">  <img src="https://github.com/lukexor/rustynes/blob/master/static/mario_bros_title.png" width="300">

## Features

The following is a checklist of features and their progress:
- [x] Console
  - [x] NTSC
  - [ ] PAL
  - [ ] Dendy
- [x] Central Processing Unit (CPU)
  - [x] Official Instructions
  - [x] Unofficial Instructions
  - [x] Interrupts
- [x] Picture Processing Unit (PPU)
  - [x] VRAM
  - [x] Background
  - [x] Sprites
  - [ ] Rastering effects
- [x] Audio Processing Unit (APU)
  - [ ] Delta Mulation Channel (DMC)
- [x] Inputs
  - [x] Keyboard (Missing Turbo)
  - [ ] Standard Controller
  - [x] Turbo support
- [x] Memory
- [x] Cartridge Support
  - [x] Battery-backed Save RAM
  - [x] iNES Format
  - [x] NES 2.0 Format (Can read headers, but many features still unsupported)
  - [x] Mappers
    - [x] NROM (Mapper 0)
    - [x] SxROM (Mapper 1)
    - [x] UxROM (Mapper 2)
    - [x] CNROM (Mapper 3)
    - [x] TxROM (Mapper 4)
    - [ ] AxROM (Mapper 7)
- [x] User Interface (UI)
  - [x] Window
  - [ ] Main Menu
  - [x] Pause
  - [x] Toggle Fullscreen
  - [x] Reset
  - [x] Power Cycle
  - [x] Increase/Decrease Speed/Fast-forward
  - [ ] Save/Load State
  - [x] Take Screenshots
  - [ ] Toggle Recording
  - [x] Toggle Sound
  - [x] Toggle Debugger
  - [ ] Custom Keybinds

## Supported Mappers

Some of the more popular mappers are implemented with more to come!

| #   | Name       | Example Games                             |
| -   | ---------- | ----------------------------------------- |
| 000 | NROM       | Bomberman, Donkey Kong, Super Mario Bros. |
| 001 | SxROM/MMC1 | Metroid, Legend of Zelda, Tetris          |
| 002 | UxROM      | Castlevania, Contra, Mega Man             |
| 003 | CNROM      | Arkanoid, Paperboy, Pipe Dream            |

## Dependencies

* [Rust](https://www.rust-lang.org/tools/install)
* [SDL2](https://www.libsdl.org/)

## Installation

While this should work on any platform that supports Rust and SDL2, it's only being developed and
tested on macOS at this time. I make no guarantees it'll work elsewhere.

* Install [Rust](https://www.rust-lang.org/tools/install)
* Install [SDL2](https://github.com/Rust-SDL2/rust-sdl2) libraries
* Download & install `RustyNES`:

        $ git clone https://github.com/lukexor/rustynes.git
        $ cd rustynes/
        $ cargo install --path ./

This will install the `RustyNES` binary to your `cargo` bin directory located at either
`$HOME/.cargo/bin/` on a Unix-like platform or `%USERPROFILE%\.cargo\bin` on Windows.

As long as that bin location is in your `$PATH` variable as outlined in the Rust installation, you
should be able to start up a game ROM following the usage below.

# Usage

```
rustynes [FLAGS] [OPTIONS] [path]

FLAGS:
    -f, --fullscreen    Fullscreen
    -h, --help          Prints help information
    -V, --version       Prints version information

OPTIONS:
    -s, --scale <scale>    Window scale (options: 1, 2, or 3) [default: 3]

ARGS:
    <path>    The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]
```

## Controls

Controller support is not implemented yet.

| Button                | Keyboard    | Controller |
| --------------------- | ----------- | ---------- |
| A                     | Z           | A          |
| B                     | X           | B          |
| A (Turbo)             | A           | X          |
| B (Turbo)             | S           | Y          |
| Start                 | Enter       | Start      |
| Select                | Right Shift | Select     |
| Up, Down, Left, Right | Arrow Keys  | D-Pad      |

There are also some emulator actions:

| Action                | Keyboard         | Controller         |
| --------------------- | ---------------- | ------------------ |
| Pause / Open Menu     | Escape           | Left Stick Button  |
| Quit                  | Ctrl-Q           |                    |
| Reset                 | Ctrl-R           |                    |
| Power Cycle           | Ctrl-P           |                    |
| Increase Speed 25%    | Ctrl-=           |                    |
| Decrease Speed 25%    | Ctrl--           |                    |
| Fast-Forward          | Space            | Right Stick Button |
| Save State            | Ctrl-(1-4)       |                    |
| Load State            | Ctrl-Shift-(1-4) |                    |
| Toggle Sound          | Ctrl-S           |                    |
| Toggle Fullscreen     | Ctrl-F           |                    |
| Take Screenshot       | Ctrl-C           |                    |
| Toggle Recording      | Ctrl-V           |                    |
| Toggle Debugger       | Ctrl-D           |                    |
| Cycle Log Level       | Ctrl-L           |                    |

### Note on Controls

Ctrl-(1-4) may have conflicts in macOS with switching Desktops 1-4. You can disable this in the
keyboard settings. I may consider changing them to something else or making macOS use the Option key
in place of Ctrl, but I'm not bothering with OS-specific bindings just yet.

## Building/Testing

To build the project run `cargo build` or `cargo build --release` (if you want playable framerates).

Unit and integration tests can be run with `cargo test`. There are also several test roms that can
be run to test various capabilities of the emulator. They are all located in the `tests/` directory.

Run them the same way you would run a game. e.g.

```
cargo run --release tests/cpu/nestest.nes
```

## Known Issues

* Many - not much works yet besides a few title screens and some test ROMs

## Documentation

* [NES Documentation (PDF)](http://nesdev.com/NESDoc.pdf)
* [NES Reference Guide (Wiki)](http://wiki.nesdev.com/w/index.php/NES_reference_guide)

## License

`RustyNES` is licensed under the GPLv3 license. See the `LICENSE.md` file in the root for a copy.

## Contact

For issue reporting, please use the github issue trackeer.

## Contributing

While this is primarily a personal project, I welcome any contributions or advice. Feel free
to submit a pull request if you want to help out!

## Credits

Implementation was inspiried by several NES projects:
- https://github.com/fogleman/nes
- https://github.com/pcwalton/sprocketnes
- https://github.com/MichaelBurge/nes-emulator
- https://github.com/AndreaOrru/LaiNES
