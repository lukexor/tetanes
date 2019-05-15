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
- [x] Central Processing Unit (CPU)
  - [x] Official Instructions
  - [ ] Unofficial Instructions (mostly done)
  - [ ] Interrupts (done but not fully functional)
- [x] Picture Processing Unit (PPU)
  - [x] VRAM
  - [ ] Interrupts (done but not fully functional)
  - [x] Background
  - [ ] Sprites (implemented, but not working correctly)
- [ ] Audio Processing Unit (APU)
- [ ] Inputs
  - [x] Keyboard (Missing Turbo)
  - [ ] Standard NES Controller
- [x] Memory
- [ ] Cartridge Support
  - [x] iNES Format
  - [ ] NES 2.0 Format
  - [ ] Mappers
    - [x] NROM (Mapper 0)
    - [x] SxROM (Mapper 1)
    - [ ] UxROM (Mapper 2)
    - [x] CNROM (Mapper 3)
    - [ ] TxROM (Mapper 4)
    - [ ] AxROM (Mapper 7)
- [ ] User Interface (UI)
  - [x] Window
  - [ ] Fullscreen
  - [ ] Menu
  - [ ] Save/Load State
  - [ ] Recording/Screenshots
  - [ ] FPS/Debug Overlay

## Dependencies

* [Rust](https://www.rust-lang.org/tools/install)
* [SDL2](https://www.libsdl.org/)

## Installation

While this should work on any platform that supports Rust and SDL2, it's only being developed and
tested on Mac OS X at this time. I make no guarantees it'll work elsewhere.

* Install [Rust](https://www.rust-lang.org/tools/install)
* Install [SDL2](https://github.com/Rust-SDL2/rust-sdl2) libraries
* Download & install `RustyNES`:

        $ git clone https://github.com/lukexor/rustynes.git
        $ cd rustynes/
        $ cargo install --path ./

This will install the `RustyNES` binary to your `cargo` bin directory located at either
`$HOME/.cargo/bin/` on a Unix-like platform or `%USERPROFILE%\.cargo\bin` on Windows.

As long as that bin location is in your '$PATH' variable as outlined in the Rust installation, you
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

### Joypad

Not yet implemented

### Keyboard

| Nintendo              | RustyNES    |
| --------------------- | ----------- |
| Up, Down, Left, Right | Arrow Keys  |
| Start                 | Enter       |
| Select                | Right Shift |
| A                     | Z           |
| B                     | X           |
| Reset                 | R           |
| Quit                  | Escape      |

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
