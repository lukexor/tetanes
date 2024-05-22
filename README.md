<!-- markdownlint-disable no-inline-html -->

# TetaNES

[![Build Status]][build] [![Doc Status]][docs] [![Latest Version]][crates.io]
[![Downloads]][crates.io] [![License]][gnu]

[build status]: https://img.shields.io/github/actions/workflow/status/lukexor/tetanes/ci.yml?branch=main
[build]: https://github.com/lukexor/tetanes/actions/workflows/ci.yml
[doc status]: https://img.shields.io/docsrs/tetanes?style=plastic
[docs]: https://docs.rs/tetanes/
[codecov]: https://codecov.io/gh/lukexor/tetanes/branch/main/graph/badge.svg?token=AMQJJ7B0LS
[coverage]: https://codecov.io/gh/lukexor/tetanes
[latest version]: https://img.shields.io/crates/v/tetanes?style=plastic
[crates.io]: https://crates.io/crates/tetanes
[downloads]: https://img.shields.io/crates/d/tetanes?style=plastic
[license]: https://img.shields.io/crates/l/tetanes?style=plastic
[gnu]: https://github.com/lukexor/tetanes/blob/main/LICENSE-MIT

<!-- markdownlint-disable line-length -->
📖 [Summary](#summary) - ✨ [Features](#features) - 🌆 [Screenshots](#screenshots) - 🚀 [Getting
Started](#getting-started) - 🛠️ [Roadmap](#roadmap) - ⚠️ [Known
Issues](#known-issues) - 💬 [Contact](#contact)
<!-- markdownlint-enable line-length -->

## Summary

<img width="100%" alt="TetaNES"
  src="https://raw.githubusercontent.com/lukexor/tetanes/main/static/tetanes.png">

> photo credit for background: [Zsolt Palatinus](https://unsplash.com/@sunitalap)
> on [unsplash](https://unsplash.com/photos/pEK3AbP8wa4)

`TetaNES` is a cross-platform emulator for the Nintendo Entertainment System
(NES) released in Japan in 1983 and North America in 1986, written in
[Rust][] using [wgpu][]. It runs on Linux, macOS, Windows, and in a web browser
with [Web Assembly][].

It started as a personal curiosity that turned into a passion project. It is
still being actively developed with new features and improvements constantly
being added. It is a fairly accurate emulator that can play most NES titles.

`TetaNES` is also meant to showcase using Rust's performance, memory safety, and
fearless concurrency features in a large project. Features used in this project
include complex enums, traits, generics, matching, iterators, channels, and
threads.

`TetaNES` also compiles for the web! Try it out in your
[browser](http://lukeworks.tech/tetanes-web)!

## Features

- Runs on Linux, macOS, Windows, and Web.
- Standalone emulation core in `tetanes-core`.
- NTSC, PAL and Dendy emulation.
- Headless Mode when using `tetanes-core`.
- Pixellate and NTSC filters.
- Up to 4 players with gamepad support.
- Zapper (Light Gun) support using the mouse.
- iNES and NES 2.0 ROM header formats supported.
- 14 supported mappers covering ~85% of licensed games.
- Game Genie Codes.
- Configurable while running using [egui](https://egui.rs).
  - Increase/Decrease speed & Fast Forward
  - Visual & Instant Rewind
  - Save & Load States
  - Battery-backed RAM saves
  - Screenshots
  - Gameplay recording and playback
  - Audio recording

## Screenshots

<img width="48%" alt="Donkey Kong"
  src="https://raw.githubusercontent.com/lukexor/tetanes/main/static/donkey_kong.png">&nbsp;&nbsp;<img
  width="48%" alt="Super Mario Bros."
  src="https://raw.githubusercontent.com/lukexor/tetanes/main/static/super_mario_bros.png">
<img width="48%" alt="The Legend of Zelda"
  src="https://raw.githubusercontent.com/lukexor/tetanes/main/static/legend_of_zelda.png">&nbsp;&nbsp;<img
  width="48%" alt="Metroid"
  src="https://raw.githubusercontent.com/lukexor/tetanes/main/static/metroid.png">

## Getting Started

`TetaNES` runs on all major operating systems (Linux, macOS, Windows, and the
web). Installable binaries will be available when `1.0.0` is released, but for the
time being you can install with `cargo` which comes installed with [Rust][].

### Install

There are multiple options for installation, depending on your operating system,
preference and existing tooling.

#### Linux

##### Ubuntu/Debian

A `.deb` package is provided under `Assets` on the latest [Release][]. Once
downloaded, you can install it and the required dependencies. e.g.

```sh
sudo apt install ./tetanes-0.10.0-1-amd64.deb
```

##### Other Distros

An [AppImage](https://appimage.org/) is provided  under `Assets` on the latest
[Release][]. Simply download it and put it wherever you want.

A `.tar.gz` package is also provided under `Assets` on the latest
[Release][]. You can place the `tetanes` binary anywhere in your `PATH`.

The following dependencies are required to be installed:

- ALSA Shared Library
- GTK3

e.g.

`apt install libasound2 libgtk-3-0`
`dnf install alsa-lib gtk3`
`pacman -Sy alsa-lib gtk3`

#### MacOS

##### App Bundle

The easiest is to download the correct app bundle for your processor. The `.dmg`
downloads can be found under the `Assets` section of the latest
[Release][].

##### Homebrew

`TetaNES` can also be installed through [Homebrew](https://brew.sh/).

```sh
brew install lukexor/formulae/tetanes
```

#### Windows

A windows installer is provided under `Assets` on the latest [Release][].

#### Cargo Install

You can also build and install with `cargo` which comes with [rustup](https://www.rust-lang.org/tools/install).

```sh
cargo install tetanes
```

This will install the latest released version of the `TetaNES` binary to your
`cargo` bin directory located at either `$HOME/.cargo/bin/` on a Unix-like
platform or `%USERPROFILE%\.cargo\bin` on Windows.

Alternatively, if you have [`cargo binstall`](https://crates.io/crates/cargo-binstall/) installed:

```sh
cargo binstall tetanes
```

This will try to find the target binary for your platform from the latest
[Release][] or install from source, similar to above.

### Usage

```text
Usage: tetanes [OPTIONS] [PATH]

Arguments:
  [PATH]  The NES ROM to load or a directory containing `.nes` ROM files.
  [default: current directory]

Options:
      --rewind                     Enable rewinding
  -s, --silent                     Silence audio
  -f, --fullscreen                 Start fullscreen
  -4, --four-player <FOUR_PLAYER>  Set four player adapter. [default: 'disabled']
                                   [possible values: disabled, four-score, satellite]
  -z, --zapper                     Enable zapper gun
      --no-threaded                Disable multi-threaded
  -m, --ram-state <RAM_STATE>      Choose power-up RAM state. [default: "all-zeros"]
                                   [possible values: all-zeros, all-ones, random]
  -r, --region <REGION>            Choose default NES region. [default: "ntsc"]
                                   [possible values: ntsc, pal, dendy]
  -i, --save-slot <SAVE_SLOT>      Save slot. [default: 1]
      --no-load                    Don't load save state on start
      --no-save                    Don't auto save state or save on exit
  -x, --speed <SPEED>              Emulation speed. [default: 1.0]
  -g, --genie-code <GENIE_CODE>    Add Game Genie Code(s). e.g. `AATOZE`
                                   (Start Super Mario Bros. with 9 lives)
      --config <CONFIG>            Custom Config path
  -c, --clean                      "Default Config" (skip user config and previous
                                   save states)
  -d, --debug                      Start with debugger open
  -h, --help                       Print help
  -V, --version                    Print version
```

[iNES][] and [NES 2.0][] formatted ROMS are supported, though some advanced `NES
2.0` features may not be implemented.

[ines]: https://wiki.nesdev.com/w/index.php/INES
[nes 2.0]: https://wiki.nesdev.com/w/index.php/NES_2.0

### Supported Mappers

Support for the following mappers is currently implemented or in development:

<!-- markdownlint-disable line-length -->

| #   | Name                 | Example Games                             | # of Games<sup>1</sup> | % of Games<sup>1</sup> |
| --- | -------------------- | ----------------------------------------- | ---------------------- | ---------------------- |
| 000 | NROM                 | Bomberman, Donkey Kong, Super Mario Bros. | ~247                   | ~10%                   |
| 001 | SxROM/MMC1B/C        | Metroid, Legend of Zelda, Tetris          | ~680                   | ~28%                   |
| 002 | UxROM                | Castlevania, Contra, Mega Man             | ~270                   | ~11%                   |
| 003 | CNROM                | Arkanoid, Paperboy, Pipe Dream            | ~155                   | ~6%                    |
| 004 | TxROM/MMC3/MMC6      | Kirby's Adventure, Super Mario Bros. 2/3  | ~599                   | ~24%                   |
| 005 | ExROM/MMC5           | Castlevania 3, Laser Invasion             | ~24                    | &lt;0.01%              |
| 007 | AxROM                | Battletoads, Marble Madness               | ~75                    | ~3%                    |
| 009 | PxROM/MMC2           | Punch Out!!                               | 1                      | &lt;0.01%              |
| 010 | FxROM/MMC4           | Fire Emblem Gaiden                        | 3                      | &lt;0.01%              |
| 011 | Color Dreams         | Crystal Mines, Metal Fighter              | 34                     | ~1%                    |
| 024 | VRC6a                | Akumajou Densetsu                         | 1                      | &lt;0.01%              |
| 026 | VRC6b                | Madara, Esper Dream 2                     | 2                      | &lt;0.01%              |
| 034 | BNROM/NINA-001       | Deadly Towers, Impossible Mission II      | 3                      | &lt;0.01%              |
| 066 | GxROM/MxROM          | Super Mario Bros. + Duck Hunt             | ~17                    | &lt;0.01%              |
| 071 | Camerica/Codemasters | Firehawk, Bee 52, MiG 29 - Soviet Fighter | ~15                    | &lt;0.01%              |
| 155 | SxROM/MMC1A          | Tatakae!! Ramen Man: Sakuretsu Choujin    | 2                      | &lt;0.01%              |
|     |                      |                                           | ~2128 / 2447           | ~87.0%                 |

<!-- markdownlint-enable line-length -->

1. [Source](http://bootgod.dyndns.org:7777/stats.php?page=6) [Mirror](https://nescartdb.com/)

### Controls

Keybindings can be customized in the keybindings menu. Below are the defaults.

NES joypad:

| Button    | Keyboard (P1) | Keyboard (P2) | Controller       |
| --------- | ------------- | ------------- | ---------------- |
| A         | Z             | N             | East             |
| B         | X             | M             | South            |
| A (Turbo) | A             |               | North            |
| B (Turbo) | S             |               | West             |
| Start     | Q             | 8             | Start            |
| Select    | W             | 9             | Select           |
| D-Pad     | Arrow Keys    | IJKL          | D-Pad            |

Controller Layout:

SDL-compatible mappings are used:
<https://github.com/mdqinc/SDL_GameControllerDB?tab=readme-ov-file> but can be
overriden by setting `SDL_GAMECONTROLLERCONFIG`.

```text
           Left Triggers                        Right Triggers
              _=====_                               _=====_
             / _____ \                             / _____ \
           +.-'_____'-.---------------------------.-'     '-.+
          /   |     |  '.                       .'            \
         / ___| /|\ |___ \                     /      (N)      \
        / |      |      | ;    _         _    ;                 ;  Action Pad
 D-Pad  | | <---   ---> | |  <:_|       |_:>  |  (W)       (E)  |  (South, East,
        | |___   |   ___| ;  Select    Start  ;                 ;   North, West)
        |\    | \|/ |    /  _              _   \      (S)      /|
        | \   |_____|  .','" "',        ,'" "', '.           .' |
        |  '-.______.-' / Left  \------/ Right \  '-._____.-'   |
        |              /\ Stick /      \ Stick /\               |
        |             /  '.___.'        '.___.'  \              |
        |            /                            \             |
         \          /                              \           /
          \________/                                \_________/
```

Emulator shortcuts:

| Action                        | Keyboard     | Controller     |
| ----------------------------- | ------------ | -------------- |
| Pause                         | Escape       | Guide Button   |
| About TetaNES                 | F1           |                |
| Configuration Menu            | Ctrl-P or F2 |                |
| Load/Open ROM                 | Ctrl-O or F3 |                |
| Quit                          | Ctrl-Q       |                |
| Reset                         | Ctrl-R       |                |
| Power Cycle                   | Ctrl-H       |                |
| Increase Speed by 25%         | =            | Right Shoulder |
| Decrease Speed by 25%         | -            | Left Shoulder  |
| Increase Scale                | Shift-=      |                |
| Decrease Scale                | Shift--      |                |
| Increase UI Scale             | Ctrl-=       |                |
| Decrease UI Scale             | Ctrl--       |                |
| Fast-Forward 2x               | Space (Hold) |                |
| Set Save State Slot (1-4)     | Ctrl-(1-4)   |                |
| Save State                    | Ctrl-S       |                |
| Load State                    | Ctrl-L       |                |
| Instant Rewind                | R (Tap)      |                |
| Visual Rewind                 | R (Hold)     |                |
| Take Screenshot               | F10          |                |
| Toggle Gameplay Recording     | Shift-V      |                |
| Toggle Audio Recording        | Shift-R      |                |
| Toggle Audio                  | Ctrl-M       |                |
| Toggle Pulse Channel 1        | Shift-1      |                |
| Toggle Pulse Channel 2        | Shift-2      |                |
| Toggle Triangle Channel       | Shift-3      |                |
| Toggle Noise Channel          | Shift-4      |                |
| Toggle DMC Channel            | Shift-5      |                |
| Toggle Fullscreen             | Ctrl-Enter   |                |
| Toggle NTSC Filter            | Ctrl-N       |                |
| Toggle CPU Debugger           | Shift-D      |                |
| Toggle PPU Debugger           | Shift-P      |                |
| Toggle APU Debugger           | Shift-A      |                |

While the CPU Debugger is open:

| Action                        | Keyboard |
| ----------------------------- | -------- |
| Step a single CPU instruction | C        |
| Step over a function          | O        |
| Step out of a function        | Shift-O  |
| Step a single scanline        | Shift-L  |
| Step an entire frame          | Shift-F  |

While the PPU Debugger is open:

| Action                         | Keyboard        |
| ------------------------------ | --------------- |
| Move debug scanline up by 1    | Ctrl-Up         |
| Move debug scanline up by 10   | Ctrl-Shift-Up   |
| Move debug scanline down by 1  | Ctrl-Down       |
| Move debug scanline down by 10 | Ctrl-Shift-Down |

Other mappings can be found and modified in the `Config -> Keybinds` menu.

### Directories

`TetaNES` stores to files to support a number of features, and depending on the
file type and varies based on operating system.

#### Configuration Preferences

- Linux: `$HOME/.config`
- macOS: `$HOME/Library/Application Support`
- Windows: `%LOCALAPPDATA%\tetanes`
- Web: Does not currently support persisting configuration preferences.

#### Screenshots

- Linux, macOS, & Windows: `$HOME/Pictures`
- Web: Does not currently support saving screenshots.

#### Replay Recordings

- Linux, macOS, & Windows: `$HOME/Documents`
- Web: Does not currently support saving recordings.

#### Audio Recordings

- Linux, macOS, & Windows: `$HOME/Music`
- Web: Does not currently support saving recordings.

#### Battery-backed RAM, save states, and logs

- Linux: `$HOME/.local/share/tetanes`
- macOS: `$HOME/Library/Application Support/tetanes`
- Windows: `%LOCALAPPDATA%\tetanes`
- Web: Does not currently support save states.

### Powerup State

The original NES hardware had semi-random contents located in RAM upon power-up
and several games made use of this to seed their Random Number Generators
(RNGs). By default, `TetaNES` honors the original hardware and emulates
randomized powerup RAM state. This shows up in several games such as `Final
Fantasy`, `River City Ransom`, and `Impossible Mission II`, amongst others. Not
emulating this would make these games seem deterministic when they weren't
intended to be.

If you would like `TetaNES` to provide fully deterministic emulated power-up
state, you'll need to change the `ram_state` setting in the configuration menu
and trigger a power-cycle or use the `-m`/`--ram_state` flag from the command
line.

### Building/Running

To build/run `TetaNES`, you'll need a nightly version of the compiler and run
`cargo build` or `cargo build --release` (if you want better framerates).

To run the web version, you'll also need the `wasm32-unknown-unknown` target and
[trunk](https://trunkrs.dev/) installed:

```sh
rustup target add wasm32-unknown-unknown
trunk serve --release
```

Unit and integration tests can be run with `cargo test`. There are also several
test roms that can be run to test various capabilities of the emulator. They are
all located in the `tetanes-core/tests_roms/` directory.

Run them in a similar way you would run a game. e.g.

```sh
cargo run --release tetanes-core/test_roms/cpu/nestest.nes
```

#### Feature Flags

- **cycle-accurate** - Enables cycle-accurate emulation. More CPU intensive, but
  supports a wider range of games requiring precise timing. Disabling may
  improve performance on lower-end machines. Enabled by default.
- **profiling** - Enables [puffin](https://github.com/EmbarkStudios/puffin)
  profiling.

### Troubleshooting

If you get an error running a ROM that's using the supported Mapper list above,
it could be a corrupted or incompatible ROM format. If you're unsure which games
use which mappers, see <http://bootgod.dyndns.org:7777/>. Trying other
versions of the same game from different sources sometimes resolves the issue.

If you get some other error when trying to start a game that previously
worked, try removing any saved states from the directories listed above to
ensure it's not an incompatible savestate file causing the issue.

If you encounter any shortcuts not working, ensure your operating system does
not have a binding for it that is overriding it. macOS specifically has many
things bound to `Ctrl-*`.

If an an issue is not already created, please use the [github issue tracker][]
to create it. A good guideline for what to include is:

- The game experiencing the issue (e.g. `Super Mario Bros 3`). Please don't
  include any download links or ROM attachments.
- Operating system and version (e.g. Windows 7, macOS Mojave 10.14.6, etc)
- What you were doing when the error happened
- A description of the error and what happeneed
- Any screenshots or console output
- Any related errors or logs

When using the web version in the browser, also include:

- Web browser and version (e.g. Chrome 77.0.3865)

## Roadmap

See [ROADMAP.md][].

## Known Issues

See the [github issue tracker][].

## Documentation

In addition to the wealth of information in the `docs/` directory, I also
referenced these websites extensively during development:

- [NES Documentation (PDF)](http://nesdev.com/NESDoc.pdf)
- [NES Dev Wiki](http://wiki.nesdev.com/w/index.php/Nesdev_Wiki)
- [6502 Datasheet](http://archive.6502.org/datasheets/rockwell_r650x_r651x.pdf)

## License

`TetaNES` is licensed under a MIT or Apache-2.0 license. See the `LICENSE-MIT`
or `LICENSE-APACHE` file in the root for a copy.

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

[rust]: https://www.rust-lang.org/
[wgpu]: https://wgpu.rs/
[web assembly]: https://webassembly.org/
[github issue tracker]: https://github.com/lukexor/tetanes/issues
[ROADMAP.md]: ROADMAP.md
[Release]: https://github.com/lukexor/tetanes/releases/latest
