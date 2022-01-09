# tetanes

## Summary

<p align="center">
  <img src="https://github.com/lukexor/tetanes/blob/main/static/tetanes.png?raw=true" width=90%">
</p>

> photo credit for background: [Zsolt Palatinus](https://unsplash.com/@sunitalap) on [unsplash](https://unsplash.com/photos/pEK3AbP8wa4)

`TetaNES` is an emulator for the Nintendo Entertainment System (NES) released in
1983, written using [Rust][], [SDL2][] and [WASM][].

It started as a personal curiosity that turned into a passion project. It is
still a work-in-progress, but I hope to transform it into a fully-featured NES
emulator that can play most games as accurately as possible. It is my hope to
see a Rust emulator rise in popularity and compete with the more popular C and
C++ versions.

`TetaNES` is also meant to showcase how clean and readable low-level Rust
programs can be in addition to them having the type and memory-safety guarantees
that Rust is known for. Many features of Rust are leveraged in this project
including traits, trait objects, generics, matching, and iterators.

Try it out in your [browser](http://dev.lukeworks.tech/tetanes)!

## Screenshots

<div style="display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between">
  <div style="flex: 0 50%; padding: 10px; min-width: 300px"><img width="100%" alt="Donkey Kong" src="https://github.com/lukexor/tetanes/blob/main/static/donkey_kong.png?raw=true"></div>
  <div style="flex: 0 50%; padding: 10px; min-width: 300px"><img width="100%" alt="Super Mario Bros." src="https://github.com/lukexor/tetanes/blob/main/static/super_mario_bros.png?raw=true"></div>
  <div style="flex: 0 50%; padding: 10px; min-width: 300px"><img width="100%" alt="The Legend of Zelda" src="https://github.com/lukexor/tetanes/blob/main/static/legend_of_zelda.png?raw=true"></div>
  <div style="flex: 0 50%; padding: 10px; min-width: 300px"><img width="100%" alt="Metroid" src="https://github.com/lukexor/tetanes/blob/main/static/metroid.png?raw=true"></div>
</div>

## Mappers

Support for the following mappers is currently implemented or in development:

| #   | Name                   | Example Games                             | # of Games<sup>1</sup>  | % of Games<sup>1</sup> |
| --- | ---------------------- | ----------------------------------------- | ----------------------- | ---------------------- |
| 000 | NROM                   | Bomberman, Donkey Kong, Super Mario Bros. |  ~247                   |                 10.14% |
| 001 | SxROM/MMC1             | Metroid, Legend of Zelda, Tetris          |  ~680                   |                 27.91% |
| 002 | UxROM                  | Castlevania, Contra, Mega Man             |  ~269                   |                 11.04% |
| 003 | CNROM                  | Arkanoid, Paperboy, Pipe Dream            |  ~155                   |                  6.36% |
| 004 | TxROM/MMC3             | Kirby's Adventure, Super Mario Bros. 2/3  |  ~599                   |                 24.59% |
| 005 | ExROM/MMC5             | Castlevania 3, Laser Invasion             |   ~24                   |                  0.99% |
| 007 | AxROM                  | Battletoads, Marble Madness               |   ~75                   |                  3.08% |
| 009 | PxROM/MMC2             | Punch Out!!                               |     1                   |              &lt;0.01% |
|     |                        |                                           | ~2050                   |                 84.11% |

1. [Source](http://bootgod.dyndns.org:7777/stats.php?page=6)

## Dependencies

* [Rust][]
* [SDL2][]

There are two methods for linking to `SDL2` libraries when building or
installing `TetaNES` from source:

- Static linking (the default).
- Dynamic linking with the `--no-default-features` feature flag. You'll need to
  manually install them using one of the methods outlined in the [rust-sdl2][]
  crate.

## Installation

This should run on most platforms that support `Rust` and `SDL2`, however, it's
only being developed on macOS at this time.

### Static Linking

```sh
cargo install cargo-vcpkg
cargo vcpkg build
cargo install tetanes
```

### Dynamic Linking

```sh
cargo install tetanes --no-default-features
```

This will install the latest version of the `TetaNES` binary to your `cargo` bin
directory located at either `$HOME/.cargo/bin/` on a Unix-like platform or
`%USERPROFILE%\.cargo\bin` on Windows.

## Usage

```text
USAGE:
    tetanes [FLAGS] [OPTIONS] [path]

FLAGS:
        --consistent_ram    Power up with consistent ram state.
    -f, --fullscreen        Start fullscreen.
    -h, --help              Prints help information
    -V, --version           Prints version information

OPTIONS:
        --speed <speed>    Emulation speed. [default: 1.0]
    -s, --scale <scale>    Window scale. [default: 3.0]

ARGS:
    <path>    The NES ROM to load, a directory containing `.nes` ROM files, or a recording playback `.playback`
              file. [default: current directory]
```

[iNES](https://wiki.nesdev.com/w/index.php/INES) and [NES
2.0](https://wiki.nesdev.com/w/index.php/NES_2.0) formats are fully supported.

## Controls

Keybindings can be customized in the configuration menu. Below are the defaults.

NES gamepad:

| Button                | Keyboard    | Controller       |
| --------------------- | ----------- | ---------------- |
| A                     | Z           | A                |
| B                     | X           | B                |
| A (Turbo)             | A           | X                |
| B (Turbo)             | S           | Y                |
| Start                 | Return      | Start            |
| Select                | Right Shift | Back             |
| Up, Down, Left, Right | Arrow Keys  | Left Stick/D-Pad |

Emulator shortcuts:

| Action                            | Keyboard         | Controller         |
| --------------------------------- | ---------------- | ------------------ |
| Help Menu                         | Ctrl-H or F1     |                    |
| Configuration Menu                | Ctrl-C or F2     |                    |
| Load/Open ROM                     | Ctrl-O or F3     |                    |
| Pause                             | Escape           | Guide Button       |
| Quit                              | Ctrl-Q           |                    |
| Reset                             | Ctrl-R           |                    |
| Power Cycle                       | Ctrl-P           |                    |
| Increase Speed by 25%             | Ctrl-=           | Right Shoulder     |
| Decrease Speed by 25%             | Ctrl--           | Left Shoulder      |
| Fast-Forward 2x (while held)      | Space            |                    |
| Set Save State Slot #             | Ctrl-(1-4)       |                    |
| Save State                        | Ctrl-S           |                    |
| Load State                        | Ctrl-L           |                    |
| Instant Rewind                    | R                |                    |
| Visual Rewind (while holding)     | R                |                    |
| Take Screenshot                   | F10              |                    |
| Toggle Gameplay Recording         | Shift-V          |                    |
| Toggle Music/Sound Recording      | Shift-R          |                    |
| Toggle Music/Sound                | Ctrl-M           |                    |
| Toggle Pulse Channel 1            | Shift-1          |                    |
| Toggle Pulse Channel 2            | Shift-2          |                    |
| Toggle Triangle Channel           | Shift-3          |                    |
| Toggle Noise Channel              | Shift-4          |                    |
| Toggle DMC Channel                | Shift-5          |                    |
| Toggle Fullscreen                 | Ctrl-Return      |                    |
| Toggle Vsync                      | Ctrl-V           |                    |
| Toggle NTSC Filter                | Ctrl-N           |                    |
| Toggle CPU Debugger               | Ctrl-D           |                    |
| Toggle PPU Viewer                 | Shift-P          |                    |
| Toggle Nametable Viewer           | Shift-N          |                    |

While the CPU Debugger is open (these can also be held down):

| Action                            | Keyboard         |
| --------------------------------- | ---------------- |
| Step a single CPU instruction     | C                |
| Step over a CPU instruction       | O                |
| Step out of a CPU instruction     | Shift-O          |
| Step a single scanline            | L                |
| Step an entire frame              | F                |
| Move Viewer scanline up           | Shift-Up         |
| Move Viewer scanline down         | Shift-Down       |

## Directories & Screenshots

Battery-backed game data and save states are stored in
`$HOME/.tetanes`. Screenshots are saved to the directory where `TetaNES` was
launched from.

## Powerup State

The original NES hardware had semi-random contents located in RAM upon powerup
and several games made use of this to seed their Random Number Generators
(RNGs). By default, `TetaNES` honors the original hardware and emulates
randomized powerup RAM state. This shows up in several games such as `Final
Fantasy`, `River City Ransom`, and `Impossible Mission II`, amongst others. Not
emulating this would make these games seem deterministic when they weren't
intended to be.

If you would like `TetaNES` to provide fully deterministic emulated powerup
state, you'll need to enable the `Consistent RAM` setting in the configuration
menu and trigger a power-cycle or use the `--consistent_ram` flag from the
command line.

## Building/Testing

To build the project run `cargo build` or `cargo build --release` (if you want
better framerates). There is also a optimized dev profile you can use which
strikes a balance between build time and performance: `cargo build --profile
dev-opt`. You may need to install SDL2 libraries, see the `Installation` section
above for options.

Unit and integration tests can be run with `cargo test`. There are also several
test roms that can be run to test various capabilities of the emulator. They are
all located in the `tests_roms/` directory.

Run them in a similar way you would run a game. e.g.

```text
$ cargo run --release test_roms/cpu/nestest.nes
```

## Debugging

There are built-in debugging tools that allow you to monitor game state and step
through CPU instructions manually. See the `Controls` section for more on
keybindings.

The default debugger screen provides CPU information such as the status of the
CPU register flags, Program Counter, Stack, PPU information, and the
previous/upcoming CPU instructions.

The Nametable Viewer displays the current Nametables in PPU memory and allows
you to scroll up/down to change the scanline at which the nametable is
read. Some games swap out nametables mid-frame.

The PPU Viewer shows the current sprite and palettes loaded. You can also scroll
up/down in a similar manner to the Nametable Viewer. `Super Mario Bros 3` for
example swaps out sprites mid-frame to render animations.

<img src="https://github.com/lukexor/tetanes/blob/main/static/nametable_viewer.png?raw=true" width="49%">&nbsp;&nbsp;<img src="https://github.com/lukexor/tetanes/blob/main/static/ppu_viewer.png?raw=true" width="49%">
<img src="https://github.com/lukexor/tetanes/blob/main/static/debugger.png?raw=true" width="100%">

Logging can be set by setting the `RUST_LOG` environment variable and setting it
to one of `trace`, `debug`, `info`, `warn` or `error` prior to building the
binary. e.g. `RUST_LOG=debug cargo build --release`

## Troubleshooting

If you get an error running a ROM that's using the supported Mapper list above,
it could be a corrupted or incompatible ROM format. If you're unsure which games
use which mappers, see <http://bootgod.dyndns.org:7777/>. Trying other
versions of the same game from different sources sometimes resolves the issue.

If you get some sort of other error when trying to start a game that previously
worked, try removing any saved states from `$HOME/.tetanes` to ensure it's not
an incompatible savestate file causing the issue.

If you encounter any shortcuts not working, ensure your operating system does
not have a binding for it that is overriding it. macOS specifically has many
things bound to `Ctrl-*`.

If an an issue is not already created, please use the [github issue tracker][]
to create it. A good guideline for what to include is:

- The game experiencing the issue (e.g. `Super Mario Bros 3`)
- Operating system and version (e.g. Windows 7, macOS Mojave 10.14.6, etc)
- What you were doing when the error happened
- A description of the error and what happeneed
- Any screenshots or console output
- Any related errors or logs

When using the WASM version in the browser, also include:
- Web browser and version (e.g. Chrome 77.0.3865)

### Known Issues

See the [github issue tracker][].

## Roadmap

The following is a checklist of features and their progress:
- [x] Console
  - [x] NTSC
  - [ ] PAL
  - [ ] Dendy
  - [ ] Headless mode
- [x] Central Processing Unit (CPU)
  - [x] Official Instructions
  - [x] Unofficial Instructions (Some still incorrect)
  - [x] Interrupts
- [x] Picture Processing Unit (PPU)
  - [x] VRAM
  - [x] Background
  - [x] Sprites
  - [x] NTSC TV Artifact Effects
  - [x] Emphasize RGB/Grayscale
- [x] Audio Processing Unit (APU)
  - [x] Pulse Channels
  - [x] Triangle Channels
  - [x] Noise Channels
  - [x] Delta Mulation Channel (DMC)
- [x] Inputs
  - [x] Keyboard
  - [x] Standard Controller
  - [x] Turbo
  - [ ] Zapper (Light Gun)
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
    - [x] ExROM/MMC5 (Mapper 5) (Split screen and sound is unfinished)
    - [x] AxROM (Mapper 7)
    - [x] PxROM/MMC2 (Mapper 9)
- [x] User Interface (UI)
  - [x] PixEngine (Custom graphics library for handling video and audio)
  - [ ] UI Notification messages
  - [x] SDL2
  - [x] WebAssembly (WASM) - Run TetaNES in the browser!
  - [x] Window
  - [ ] Menus
    - [ ] Help Menu
    - [x] Open/Run ROM with file browser
    - [x] Configuration options
    - [ ] Custom Keybinds
    - [ ] Recent Game Selection
  - [x] Pause
  - [x] Reset
  - [x] Power Cycle
  - [x] Increase/Decrease Speed
  - [x] Fast-forward
  - [ ] Instant Rewind (5 seconds)
  - [ ] Visual Rewind (Holding R will time-travel backward)
  - [ ] Save/Load State
  - [X] Take Screenshots
  - [ ] Toggle Gameplay Recording
  - [ ] Sound Recording (Save those memorable tunes!)
  - [x] Toggle Fullscreen
  - [x] Toggle Sound
  - [x] Toggle NTSC Filter
  - [x] Toggle individual sound channels
  - [ ] Toggle Debugger
  - [x] Game Genie
  - [ ] [WideNES](https://prilik.com/ANESE/wideNES)
  - [ ] 4-Player support
  - [ ] Network Multi-player
  - [ ] Self Updater
- [x] Testing/Debugging/Documentation
  - [ ] CPU Debugger (Displays CPU status, registers, and disassembly)
    - [ ] Step Into/Out/Over
    - [ ] Breakpoints
  - [ ] Memory Hex Debugger
  - [ ] PPU Viewer (Displays PPU sprite patterns and color palettes)
  - [ ] Nametable Viewer (Displays all four PPU backgrounds)
    - [ ] Scanline Hit Configuration (For debugging IRQ Nametable changes)
    - [ ] Scroll lines (Automatically adjusts the scanline, showing live
    nametable changes)
  - [x] Unit/Integration tests (run with cargo test)
    - [x] CPU integration testing (with [nestest](http://www.qmtpro.com/~nes/misc/nestest.txt))
    - [ ] Other tests (Missing a lot here)
  - [x] Test ROMs (most pass, many still do not)
      - [ ] Automated rom tests (in progress now that action recording is
      finished)
  - [x] Rust Docs
  - [x] Logging

## Documentation

In addition to the wealth of information in the `docs/` directory, I also
referenced these websites extensively during development:

* [NES Documentation (PDF)](http://nesdev.com/NESDoc.pdf)
* [NES Dev Wiki](http://wiki.nesdev.com/w/index.php/Nesdev_Wiki)
* [6502 Datasheet](http://archive.6502.org/datasheets/rockwell_r650x_r651x.pdf)

## License

`TetaNES` is licensed under the GPLv3 license. See the `LICENSE.md` file in the
root for a copy.

## Contribution

While this is primarily a personal project, I welcome any contributions or
suggestions. Feel free to submit a pull request if you want to help out!

### Contact

For issue reporting, please use the [github issue tracker][]. You can also
contact me directly at <https://lukeworks.tech/contact/>.

## Credits

Implementation was inspiried by several amazing NES projects, without which I
would not have been able to understand or digest all the information on the NES
wiki.

- [fogleman NES](https://github.com/fogleman/nes)
- [sprocketnes](https://github.com/pcwalton/sprocketnes)
- [nes-emulator](https://github.com/MichaelBurge/nes-emulator)
- [LaiNES](https://github.com/AndreaOrru/LaiNES)
- [ANESE](https://github.com/daniel5151/ANESE)
- [FCEUX](http://www.fceux.com/web/home.html)

I also couldn't have gotten this far without the amazing people over on the
[NES Dev Forums](http://forums.nesdev.com/):
- [blargg](http://forums.nesdev.com/memberlist.php?mode=viewprofile&u=17) for
  all his amazing [test roms](https://wiki.nesdev.com/w/index.php/Emulator_tests)
- [bisqwit](https://bisqwit.iki.fi/) for his test roms & integer NTSC video
  implementation
- [Disch](http://forums.nesdev.com/memberlist.php?mode=viewprofile&u=33)
- [Quietust](http://forums.nesdev.com/memberlist.php?mode=viewprofile&u=7)
- [rainwarrior](http://forums.nesdev.com/memberlist.php?mode=viewprofile&u=5165)
- And many others who helped me understand the stickier bits of emulation

Also, a huge shout out to
[OneLoneCoder](https://github.com/OneLoneCoder/) for his
[NES](https://github.com/OneLoneCoder/olcNES) and
[olcPixelGameEngine](https://github.com/OneLoneCoder/olcPixelGameEngine)
series as those helped a ton in some recent refactorings.

[Rust]: https://www.rust-lang.org/tools/install
[rust-sdl2]: https://github.com/Rust-SDL2/rust-sdl2#sdl20-development-libraries
[SDL2]: https://www.libsdl.org/
[WASM]: https://webassembly.org/
[github issue tracker]: https://github.com/lukexor/tetanes/issues
