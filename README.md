# RustyNES

## Summary

`RustyNES` is an emulator for the Nintendo Entertainment System (NES) released in 1983, written
using [Rust][rust] and [SDL2][sdl2].

It started as a personal curiosity that turned into a project for two classes to demonstrate
a proficiency in Rust and in digital sound production. It is still a work-in-progress, but I hope to
transform it into a fully-featured NES emulator that can play most games. It is my hope to see
a Rust emulator rise in popularity and compete with the more popular C and C++ versions.

`RustyNES` is also meant to showcase how clean and readable low-level Rust programs can be in addition
to them having the type and memory-safety guarantees that Rust is known for.

## Screenshots

<img src="https://github.com/lukexor/rustynes/blob/master/static/donkey_kong.png" width="400">&nbsp;&nbsp;<img src="https://github.com/lukexor/rustynes/blob/master/static/super_mario_bros.png" width="400">
<img src="https://github.com/lukexor/rustynes/blob/master/static/legend_of_zelda.png" width="400">&nbsp;&nbsp;<img src="https://github.com/lukexor/rustynes/blob/master/static/metroid.png" width="400">

## Supported Mappers

Some of the more popular mappers are implemented with more to come!

| #   | Name       | Example Games                                            |
| -   | ---------- | ---------------------------------------------------------|
| 000 | NROM       | Bomberman, Donkey Kong, Super Mario Bros.                |
| 001 | SxROM/MMC1 | Metroid, Legend of Zelda, Tetris                         |
| 002 | UxROM      | Castlevania, Contra, Mega Man                            |
| 003 | CNROM      | Arkanoid, Paperboy, Pipe Dream                           |
| 004 | TxROM/MMC3 | Kickle Cubicle, Kirby's Adventure, Super Mario Bros. 2/3 |

## Dependencies

* [Rust][rust]
* [SDL2][sdl2]

## Installation

While this should work on any platform that supports Rust and SDL2, it's only being developed and
tested on macOS at this time. I make no guarantees it'll work elsewhere. Send me a line if it does
work though and I'll update this section.

* Install [Rust][rust]
* Install [SDL2](https://github.com/Rust-SDL2/rust-sdl2) development libraries
  * Linux and macOS should be straightforward
  * Windows makes this a bit more complicated. Be sure to follow the above link carefully. For the simple cast using `rustup`, all of the `lib` files should go in your `C:\Users\{Your Username}\.rustup\toolchains\{current toolchain}\lib\rustlib\{current toolchain}\lib` directory (where the `{current toolchain}` will likely have `x86_64-pc-windows` in its name) and then a copy of `SDl2.dll` needs to go in your `%USERPROFILE%\.cargo\bin` directory next to the `rustynes.exe` binary.
* Download & install `RustyNES`. Stable releases can be found on the `Releases` tab at the top of
the page. To build directly from a release tag, follow these steps:

        $ git clone https://github.com/lukexor/rustynes.git
        $ cd rustynes/
        $ git checkout v0.2.0
        $ cargo install --path ./

This will install the `v0.2.0` tagged release of the `RustyNES` binary to your `cargo` bin directory located at either
`$HOME/.cargo/bin/` on a Unix-like platform or `%USERPROFILE%\.cargo\bin` on Windows. You can see which release tags are available by running this command:

        $ git tag -l

As long as that bin location is in your `$PATH` variable as outlined in the Rust install
instructions, you should be able to start up a game ROM following the usage below.

## Usage

```
rustynes [FLAGS] [OPTIONS] [path]

FLAGS:
    -d, --debug         Debug
    -f, --fullscreen    Fullscreen
    -h, --help          Prints help information
    -V, --version       Prints version information

OPTIONS:
    -l, --load <load>      Load Save State
    -s, --scale <scale>    Window scale [default: 3]

ARGS:
    <path>    The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]
```

## Controls

| Button                | Keyboard    | Controller       |
| --------------------- | ----------- | ---------------- |
| A                     | Z           | A                |
| B                     | X           | B                |
| A (Turbo)             | A           | X/Left Trigger   |
| B (Turbo)             | S           | Y/Right Trigger  |
| Start                 | Enter       | Start            |
| Select                | Right Shift | Select           |
| Up, Down, Left, Right | Arrow Keys  | Left Stick/D-Pad |

There are also some emulator actions:

| Action                | Keyboard         | Controller         |
| --------------------- | ---------------- | ------------------ |
| Open/Run ROM          | Ctrl-O           |                    |
| Pause / Open Menu     | Escape           | Left Stick Button  |
| Quit                  | Ctrl-Q           |                    |
| Reset                 | Ctrl-R           |                    |
| Power Cycle           | Ctrl-P           |                    |
| Increase Speed 25%    | Ctrl-=           |                    |
| Decrease Speed 25%    | Ctrl--           |                    |
| Toggle Fast-Forward   | Space            | Right Stick Button |
| Set State Slot        | Ctrl-(1-4)       |                    |
| Save State            | Ctrl-S           | Left Shoulder      |
| Load State            | Ctrl-L           | Right Shoulder     |
| Toggle Music/Sound    | Ctrl-M           |                    |
| Toggle Recording      | Ctrl-V           |                    |
| Toggle Debugger       | Ctrl-D           |                    |
| Toggle Fullscreen     | Ctrl-Enter       |                    |
| Take Screenshot       | F10              |                    |
| Cycle Log Level       | F9               |                    |

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

See the github issue tracker.

## Roadmap

The following is a checklist of features and their progress:
- [x] Console
  - [x] NTSC
  - [ ] PAL
  - [ ] Dendy
- [x] Central Processing Unit (CPU)
  - [x] Official Instructions
  - [x] Unofficial Instructions (Not fully tested)
  - [x] Interrupts
- [x] Picture Processing Unit (PPU)
  - [x] VRAM
  - [x] Background
  - [x] Sprites
  - [ ] TV Raster Effects
  - [ ] Emphasize RGB/Grayscale
- [x] Audio Processing Unit (APU)
  - [x] Delta Mulation Channel (DMC)
- [x] Inputs
  - [x] Keyboard
  - [x] Standard Controller
  - [x] Turbo support
- [x] Memory
- [x] Cartridge
  - [x] Battery-backed Save RAM
  - [x] iNES Format
  - [x] NES 2.0 Format (Can read headers, but many features still unsupported)
  - [x] Mappers
    - [x] NROM (Mapper 0)
    - [x] SxROM/MMC1 (Mapper 1)
    - [x] UxROM (Mapper 2)
    - [x] CNROM (Mapper 3)
    - [x] TxROM/MMC3 (Mapper 4)
    - [ ] ExROM/MMC5 (Mapper 7)
    - [ ] AxROM (Mapper 7)
    - [ ] PxROM/MMC2 (Mapper 9)
    - [ ] FxROM/MMC4 (Mapper 10)
- [x] User Interface (UI)
  - [x] Window
  - [ ] Main Menu
  - [ ] Open/Run ROM with file browser
  - [x] Pause
  - [x] Toggle Fullscreen
  - [x] Reset
  - [x] Power Cycle
  - [x] Increase/Decrease Speed/Fast-forward
  - [x] Save/Load State
  - [x] Take Screenshots
  - [ ] Toggle Recording
  - [x] Toggle Sound
  - [x] Toggle Debugger
  - [ ] Custom Keybinds
  - [ ] Rewind
  - [ ] Game Genie
  - [ ] WideNES

## Documentation

In addition to the wealth of information in the `docs` directory, I also referenced these websites
extensively during development:

* [NES Documentation (PDF)](http://nesdev.com/NESDoc.pdf)
* [NES Wiki](http://wiki.nesdev.com/w/index.php/Nesdev_Wiki)

## License

`RustyNES` is licensed under the GPLv3 license. See the `LICENSE.md` file in the root for a copy.

## Contact

For issue reporting, please use the github issue tracker. You can contact me directly
[here](https://lukeworks.tech/contact/).

## Contributing

While this is primarily a personal project, I welcome any contributions or suggestions. Feel free to
submit a pull request if you want to help out!

## Credits

Implementation was inspiried by several amazing NES projects, without which I would not have been
able to understand or digest all the information on the NES wiki.

- https://github.com/fogleman/nes
- https://github.com/pcwalton/sprocketnes
- https://github.com/MichaelBurge/nes-emulator
- https://github.com/AndreaOrru/LaiNES
- https://github.com/daniel5151/ANESE

[rust]: https://www.rust-lang.org/tools/install
[sdl2]: https://www.libsdl.org/
