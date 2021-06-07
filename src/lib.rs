#![warn(
    // missing_docs,
    unused,
    deprecated_in_future,
    unreachable_pub,
    unused_crate_dependencies,
    bare_trait_objects,
    ellipsis_inclusive_range_patterns,
    future_incompatible,
    missing_debug_implementations,
    nonstandard_style,
    rust_2018_idioms,
    trivial_casts,
    trivial_numeric_casts,
    variant_size_differences,
    rustdoc::broken_intra_doc_links,
    rustdoc::private_intra_doc_links
)]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/lukexor/tetanes/master/static/tetanes_icon.png"
)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/lukexor/tetanes/master/static/tetanes_icon.png"
)]

//! # Summary
//!
//! <p align="center">
//!   <img src="https://raw.githubusercontent.com/lukexor/tetanes/master/static/tetanes.png" width="800">
//! </p>
//!
//! > photo credit for background: [Zsolt Palatinus](https://unsplash.com/@sunitalap) on [unsplash](https://unsplash.com/photos/pEK3AbP8wa4)
//!
//! `TetaNES` is an emulator for the Nintendo Entertainment System (NES) released in 1983, written
//! using [Rust][rust], [SDL2][sdl2] and [WASM][wasm].
//!
//! It started as a personal curiosity that turned into a passion project. It is still a
//! work-in-progress, but I hope to transform it into a fully-featured NES emulator that can play
//! most games as accurately as possible. It is my hope to see a Rust emulator rise in popularity
//! and compete with the more popular C and C++ versions.
//!
//! `TetaNES` is also meant to showcase how clean and readable low-level Rust programs can be in
//! addition to them having the type and memory-safety guarantees that Rust is known for. Many
//! useful features of Rust are leveraged in this project including traits, trait objects,
//! generics, matching, and iterators.
//!
//! Try it out in your [browser](http://dev.lukeworks.tech/tetanes)!
//!
//! # Screenshots
//!
//! <img src="https://raw.githubusercontent.com/lukexor/tetanes/master/static/donkey_kong.png" width="400">&nbsp;&nbsp;<img src="https://raw.githubusercontent.com/lukexor/tetanes/master/static/super_mario_bros.png" width="400">
//! <img src="https://raw.githubusercontent.com/lukexor/tetanes/master/static/legend_of_zelda.png" width="400">&nbsp;&nbsp;<img src="https://raw.githubusercontent.com/lukexor/tetanes/master/static/metroid.png" width="400">
//!
//! # Mappers
//!
//! Support for the following mappers is currently implemented or in development:
//!
//! | #   | Name                   | Example Games                             | # of Games<sup>1</sup>  | % of Games<sup>1</sup> |
//! | --- | ---------------------- | ----------------------------------------- | ----------------------- | ---------------------- |
//! | 000 | NROM                   | Bomberman, Donkey Kong, Super Mario Bros. |  ~247                   |                 10.14% |
//! | 001 | SxROM/MMC1             | Metroid, Legend of Zelda, Tetris          |  ~680                   |                 27.91% |
//! | 002 | UxROM                  | Castlevania, Contra, Mega Man             |  ~269                   |                 11.04% |
//! | 003 | CNROM                  | Arkanoid, Paperboy, Pipe Dream            |  ~155                   |                  6.36% |
//! | 004 | TxROM/MMC3             | Kirby's Adventure, Super Mario Bros. 2/3  |  ~599                   |                 24.59% |
//! | 005 | ExROM/MMC5             | Castlevania 3, Laser Invasion             |   ~24                   |                  0.99% |
//! | 007 | AxROM                  | Battletoads, Marble Madness               |   ~75                   |                  3.08% |
//! | 009 | PxROM/MMC2             | Punch Out!!                               |     1                   |              &lt;0.01% |
//! |     |                        |                                           | ~2050                   |                 84.11% |
//!
//! 1. [Source](http://bootgod.dyndns.org:7777/stats.php?page=6)
//!
//! # Dependencies
//!
//! * [Rust][rust]
//! * [SDL2][sdl2]
//!
//! # Installation
//!
//! This should run on most platforms that supports Rust and SDL2, howeer, it's only being
//! developed and tested on macOS at this time. So far, I've tested on macOS High Sierra, Mojave,
//! Windows 7, Windows 10, Linux (Fedora and Ubuntu), and Raspberry Pi 4 (though performance
//! lacking). When 1.0.0 is released, I'll make binaries available for all major platforms. Until
//! then, follow the below instructions to build for your platform.
//!
//! * Install [Rust][rust] (follow the link)
//!   * If Rust is already installed. Ensure it's up-to-date by running:
//!
//!     ```text
//!     $ rustup update
//!     ```
//!
//! * Install [SDL2](https://raw.githubusercontent.com/Rust-SDL2/rust-sdl2) development libraries
//! (follow the link)
//!   * Linux and macOS should be straightforward
//!   * Windows makes this a bit more complicated. Be sure to follow the above link instructions
//!   carefully. For the simple case of using `rustup`, all of the files in `lib\` from the Visual
//!   C++ 32/64-bit development zip should go in:
//!
//!     ```text
//!     C:\Users\{Your Username}\.rustup\toolchains\{current toolchain}\lib\rustlib\{current toolchain}\lib
//!     ```
//!
//!     Where `{current toolchain}` will likely have `x86_64-pc-windows` in its name. then a copy
//!     of `lib\SDl2.dll` needs to go in:
//!
//!     ```text
//!     %USERPROFILE%\.cargo\bin
//!     ```
//!
//!     Next to the `tetanes.exe` binary.
//! * Download & install `TetaNES`. Stable releases can be found on the `Releases` tab at the top
//! of the page. To build directly from a release tag, follow these steps:
//!
//! ```text
//! $ git clone https://raw.githubusercontent.com/lukexor/tetanes.git
//! $ cd tetanes/
//! $ git checkout v0.6.0
//! $ cargo install --path ./
//! ```
//!
//! This will install the `v0.6.0` tagged release of the `TetaNES` binary to your `cargo` bin
//! directory located at either `$HOME/.cargo/bin/` on a Unix-like platform or
//! `%USERPROFILE%\.cargo\bin` on Windows. Replace the release tag with the one you want to
//! install. The latest is recommended. You can see which release tags are available by clicking
//! the `Releases` tab at the top of this page or by running the following command from the checked
//! out git repository:
//!
//! ```text
//! $ git tag -l
//! ```
//!
//! # Usage
//!
//! For each platform, the first `cd` command may not be needed depending on the contents of your
//! `$PATH` environment variable. `filename` should be replaced by the path to your game ROM ending
//! in `nes`.  At present, only the [iNES](https://wiki.nesdev.com/w/index.php/INES) format is
//! fully supported, but [NES 2.0](https://wiki.nesdev.com/w/index.php/NES_2.0) support is coming.
//!
//! ## Windows
//!
//! ```text
//! $ cd %USERPROFILE%\.cargo\bin
//! $ tetanes.exe {filename}
//! ```
//!
//! ## macOS/Linux
//!
//! ```text
//! $ cd $HOME/.cargo/bin/
//! $ tetanes {filename}
//! ```
//!
//! ## Additional Options
//!
//! ```text
//! USAGE:
//!     tetanes [FLAGS] [OPTIONS] [--] [path]
//!
//! FLAGS:
//!     -c, --clear-savestate    Removes existing savestates for current save-slot
//!         --concurrent-dpad    Enables the ability to simulate concurrent L+R and U+D on the D-Pad.
//!     -d, --debug              Start with the CPU debugger enabled and emulation paused at first CPU instruction.
//!     -f, --fullscreen         Start fullscreen.
//!     -h, --help               Prints help information
//!     -r, --record             Record gameplay to a file for later action replay.
//!         --rewind             Enable savestate rewinding
//!         --savestates-off     Disable savestates
//!         --sound-off          Disable sound.
//!     -V, --version            Prints version information
//!         --vsync-off          Disable vsync.
//!
//! OPTIONS:
//!     -g, --genie-codes <genie-codes>...    List of Game Genie Codes (space separated).
//!     -p, --replay <replay>                 Replay a saved action replay file.
//!         --savestate-slot <save-slot>      Set savestate slot #. [default: 1]  [possible values: 1, 2, 3, 4]
//!     -s, --scale <scale>                   Window scale [default: 3]
//!         --speed <speed>                   Increase/Decrease emulation speed. [default: 1.0]
//!
//! ARGS:
//!     <path>    The NES ROM to load or a directory containing `.nes` ROM files. [default: current directory]
//! ```
//!
//! # Controls
//!
//! | Button                | Keyboard    | Controller       |
//! | --------------------- | ----------- | ---------------- |
//! | A                     | Z           | A                |
//! | B                     | X           | B                |
//! | A (Turbo)             | A           | X                |
//! | B (Turbo)             | S           | Y                |
//! | Start                 | Enter       | Start            |
//! | Select                | Right Shift | Back             |
//! | Up, Down, Left, Right | Arrow Keys  | Left Stick/D-Pad |
//!
//! There are also some emulator actions:
//!
//! | Action                            | Keyboard         | Controller         |
//! | --------------------------------- | ---------------- | ------------------ |
//! | Pause                             | Escape           | Guide Button       |
//! | Help Menu<sup>\*</sup>            | F1               |                    |
//! | Configuration Menu<sup>\*</sup>   | Ctrl-C           |                    |
//! | Open ROM<sup>\*</sup>             | Ctrl-O           |                    |
//! | Quit                              | Ctrl-Q           |                    |
//! | Reset                             | Ctrl-R           |                    |
//! | Power Cycle                       | Ctrl-P           |                    |
//! | Increase Speed 25%                | Ctrl-=           | Right Shoulder     |
//! | Decrease Speed 25%                | Ctrl--           | Left Shoulder      |
//! | Fast-Forward 2x (while held)      | Space            |                    |
//! | Set Save State Slot #             | Ctrl-(1-4)       |                    |
//! | Save State                        | Ctrl-S           |                    |
//! | Load State                        | Ctrl-L           |                    |
//! | Rewind 5 Seconds                  | R                |                    |
//! | Stop Action Replay Recording      | Shift-V          |                    |
//! | Toggle Music/Sound                | Ctrl-M           |                    |
//! | Toggle CPU Debugger               | Ctrl-D           |                    |
//! | Toggle Fullscreen                 | Ctrl-Return      |                    |
//! | Toggle Vsync                      | Ctrl-V           |                    |
//! | Toggle NTSC Filter                | Ctrl-N           |                    |
//! | Toggle PPU Viewer                 | Shift-P          |                    |
//! | Toggle Nametable Viewer           | Shift-N          |                    |
//! | Take Screenshot                   | F10              |                    |
//!
//! While the CPU Debugger is open (these can also be held down):
//!
//! | Action                            | Keyboard         |
//! | --------------------------------- | ---------------- |
//! | Step a single CPU instruction     | C                |
//! | Step over a CPU instruction       | O                |
//! | Step out of a CPU instruction     | Ctrl-O           |
//! | Step a single scanline            | S                |
//! | Step an entire frame              | F                |
//! | Toggle Live CPU Debug Updating    | D                |
//!
//! <sup>&ast;</sup>: Not yet Implemented
//!
//! ## Note on Controls
//!
//! Ctrl-(1-4) may have conflicts in macOS with switching Desktops 1-4. You can disable this in the
//! keyboard settings. I may consider changing them to something else or making macOS use the
//! Option key in place of Ctrl, but I'm not bothering with OS-specific bindings just yet.
//!
//! # Directories & Screenshots
//!
//! Battery-backed game data and save states are stored in `$HOME/.tetanes`. Screenshots are saved
//! to the directory where `TetaNES` was launched from. This may change in a future release.
//!
//! # Powerup State
//!
//! The original NES hardware had semi-random contents located in RAM upon powerup and several
//! games made use of this to seed their Random Number Generators (RNGs). By default, `TetaNES`
//! emulates randomized powerup RAM state. This shows up in several games such as Final Fantasy,
//! River City Ransom, and Impossible Mission II, amongst others. Not emulating this would make
//! these games seem deterministic when they weren't intended to be.
//!
//! If you would like `TetaNES` to provide fully deterministic emulated powerup state, you'll need
//! to enable the `consistent_ram` configuration setting.
//!
//! # Building/Testing
//!
//! To build the project, ensure the dependencies are installed as outlined in the `Installation`
//! section and then run `cargo build` or `cargo build --release` (if you want better framerates).
//!
//! Unit and integration tests can be run with `cargo test`. There are also several test roms that
//! can be run to test various capabilities of the emulator. They are all located in the `tests/`
//! directory.
//!
//! Run them in a similar way you would run a game. e.g.
//!
//! ```text
//! $ cargo run --release tests/cpu/nestest.nes
//! ```
//!
//! # Debugging
//!
//! There are built-in debugging tools that allow you to monitor game state and step through CPU
//! instructions manually. See the `Controls` section for more on keybindings.
//!
//! The Default debugger screen provides CPU information such as the statis of the CPU register
//! flags, Program Counter, Stack, PPU information, and the previous/upcoming CPU instructions.
//!
//! The Nametable Viewer displays the current Nametables in PPU memory and allows you to scroll
//! up/down to change the scanline at which the nametable is read. Some games swap out nametables
//! mid-frame.
//!
//! The PPU Viewer shows the current sprite and palettes loaded. You can also scroll up/down in a
//! similar manner to the Nametable Viewer. Super Mario Bros 3 for example swaps out sprites
//! mid-frame to render animations.
//!
//! <img src="https://raw.githubusercontent.com/lukexor/tetanes/master/static/nametable_viewer.png" width="400">&nbsp;&nbsp;<img src="https://raw.githubusercontent.com/lukexor/tetanes/master/static/ppu_viewer.png" width="400">
//! <img src="https://raw.githubusercontent.com/lukexor/tetanes/master/static/debugger.png" width="808">
//!
//! Logging can be set by setting the `RUST_LOG` environment variable and setting it
//! to one of `trace`, `debug`, `info`, `warn` or `error`.
//!
//! # Troubleshooting
//!
//! If you get an error running a ROM that's using the supported Mapper list above, it could be a
//! corrupted or incompatible ROM format. If you're unsure which games use which mappers, see
//! [here](http://bootgod.dyndns.org:7777/). Trying other versions of the same game from different
//! sources sometimes resolves the issue.
//!
//! If you get some sort of nasty error when trying to start a game you've played before, try
//! passing the `--clear-savestate` option to ensure it's not an incompatible savestate file
//! causing the issue.  When 1.0 releases, I'll be much more careful about backwards breaking
//! changes with regards to savestate files, but for now it's highly volatile and due to the nature
//! of how I serialize data, I can only catch certain sorts of data inconsistencies.
//!
//! If you encounter any shortcuts not working, ensure your operating system does not have a
//! binding for it that is overriding it. macOs specifically has many things bound to
//! `Ctrl-*`. There are plans to allow keybind customization, but it's not finished yet.
//!
//! If an an issue is not already created, please use the github issue tracker to create it. A good
//! guideline for what to include is:
//!
//! - The game experiencing the issue (e.g. Super Mario Bros 3)
//! - Operating system and version (e.g. Windows 7, macOS Mojave 10.14.6, etc)
//! - What you were doing when the error happened
//! - A description of the error and what happeneed
//! - Any console output
//! - Any related errors or logs
//!
//! When using the browser version (not yet available), also include:
//! - Web browser and version (e.g. Chrome 77.0.3865)
//!
//! ## Known Issues
//!
//! See the github issue tracker.
//!
//! # Roadmap
//!
//! The following is a checklist of features and their progress:
//! - [x] Console
//!   - [x] NTSC
//!   - [ ] PAL
//!   - [ ] Dendy
//!   - [ ] Headless mode
//! - [x] Central Processing Unit (CPU)
//!   - [x] Official Instructions
//!   - [x] Unofficial Instructions (Some still incorrect)
//!   - [x] Interrupts
//! - [x] Picture Processing Unit (PPU)
//!   - [x] VRAM
//!   - [x] Background
//!   - [x] Sprites
//!   - [x] NTSC TV Artifact Effects
//!   - [x] Emphasize RGB/Grayscale
//! - [x] Audio Processing Unit (APU)
//!   - [x] Pulse Channels
//!   - [x] Triangle Channels
//!   - [x] Noise Channels
//!   - [x] Delta Mulation Channel (DMC)
//! - [x] Inputs
//!   - [x] Keyboard
//!   - [x] Standard Controller
//!   - [x] Turbo
//!   - [ ] Zapper (Light Gun)
//! - [x] Memory
//! - [x] Cartridge
//!   - [x] Battery-backed Save RAM
//!   - [x] iNES Format
//!   - [x] NES 2.0 Format (Can read headers, but many features still unsupported)
//!   - [x] Mappers
//!     - [x] NROM (Mapper 0)
//!     - [x] SxROM/MMC1 (Mapper 1)
//!     - [x] UxROM (Mapper 2)
//!     - [x] CNROM (Mapper 3)
//!     - [x] TxROM/MMC3 (Mapper 4)
//!     - [x] ExROM/MMC5 (Mapper 5) (Split screen and sound is unfinished)
//!     - [x] AxROM (Mapper 7)
//!     - [x] PxROM/MMC2 (Mapper 9)
//! - [x] User Interface (UI)
//!   - [x] PixEngine (Custom graphics library for handling video and audio)
//!   - [x] UI Notification messages
//!   - [x] SDL2
//!   - [x] WebAssembly (WASM) - Run TetaNES in the browser!
//!   - [x] Window
//!   - [ ] Menus
//!     - [ ] Help Menu
//!     - [ ] Open/Run ROM with file browser
//!     - [ ] Configuration options
//!     - [ ] Custom Keybinds
//!     - [ ] Recent Game Selection
//!   - [x] Pause
//!   - [x] Toggle Fullscreen
//!   - [x] Reset
//!   - [x] Power Cycle
//!   - [x] Increase/Decrease Speed/Fast-forward
//!   - [x] Instant Rewind (5 seconds)
//!   - [ ] Visual Rewind (Holding R will time-travel backward)
//!   - [x] Save/Load State
//!   - [x] Take Screenshots
//!   - [x] Toggle Action Recording
//!   - [ ] Sound Recording (Save those memorable tunes!)
//!   - [x] Toggle Sound
//!   - [x] Toggle Debugger
//!   - [x] Game Genie
//!   - [ ] [WideNES](https://prilik.com/ANESE/wideNES)
//!   - [ ] 4-Player support
//!   - [ ] Network Multi-player
//!   - [ ] Toggle individual sound channels
//!   - [ ] Self Updater
//! - [x] Testing/Debugging/Documentation
//!   - [x] CPU Debugger (Displays CPU status, registers, and disassembly)
//!     - [X] Step Into/Out/Over
//!     - [ ] Breakpoints
//!   - [ ] Memory Hex Debugger
//!   - [x] PPU Viewer (Displays PPU sprite patterns and color palettes)
//!   - [x] Nametable Viewer (Displays all four PPU backgrounds)
//!     - [X] Scanline Hit Configuration (For debugging IRQ Nametable changes)
//!     - [ ] Scroll lines (Automatically adjusts the scanline, showing live nametable changes)
//!   - [x] Unit/Integration tests (run with cargo test)
//!     - [x] CPU integration testing (with [nestest](http://www.qmtpro.com/~nes/misc/nestest.txt))
//!     - [ ] Other tests (Missing a lot here)
//!   - [x] Test ROMs (most pass, many still do not)
//!       - [ ] Automated rom tests (in progress now that action recording is finished)
//!   - [ ] Rust Docs
//!   - [ ] Logging
//!       - [x] Console
//!       - [ ] File
//!
//! # Documentation
//!
//! In addition to the wealth of information in the `docs/` directory, I also referenced these
//! websites extensively during development:
//!
//! * [NES Documentation (PDF)](http://nesdev.com/NESDoc.pdf)
//! * [NES Dev Wiki](http://wiki.nesdev.com/w/index.php/Nesdev_Wiki)
//! * [6502 Datasheet](http://archive.6502.org/datasheets/rockwell_r650x_r651x.pdf)
//!
//! # License
//!
//! `TetaNES` is licensed under the GPLv3 license. See the `LICENSE.md` file in the root for a
//! copy.
//!
//! ## Contact
//!
//! For issue reporting, please use the github issue tracker. You can contact me directly
//! [here](https://lukeworks.tech/contact/).
//!
//! # Contributing
//!
//! While this is primarily a personal project, I welcome any contributions or suggestions. Feel
//! free to submit a pull request if you want to help out!
//!
//! # Credits
//!
//! Implementation was inspiried by several amazing NES projects, without which I would not have
//! been able to understand or digest all the information on the NES wiki.
//!
//! - [fogleman NES](https://raw.githubusercontent.com/fogleman/nes)
//! - [sprocketnes](https://raw.githubusercontent.com/pcwalton/sprocketnes)
//! - [nes-emulator](https://raw.githubusercontent.com/MichaelBurge/nes-emulator)
//! - [LaiNES](https://raw.githubusercontent.com/AndreaOrru/LaiNES)
//! - [ANESE](https://raw.githubusercontent.com/daniel5151/ANESE)
//! - [FCEUX](http://www.fceux.com/web/home.html)
//!
//! I also couldn't have gotten this far without the amazing people over on the [NES Dev
//! Forums](http://forums.nesdev.com/):
//! - [blargg](http://forums.nesdev.com/memberlist.php?mode=viewprofile&u=17) for all his amazing
//!   [test roms](https://wiki.nesdev.com/w/index.php/Emulator_tests)
//! - [bisqwit](https://bisqwit.iki.fi/) for his test roms & integer NTSC video implementation
//! - [Disch](http://forums.nesdev.com/memberlist.php?mode=viewprofile&u=33)
//! - [Quietust](http://forums.nesdev.com/memberlist.php?mode=viewprofile&u=7)
//! - [rainwarrior](http://forums.nesdev.com/memberlist.php?mode=viewprofile&u=5165)
//! - And many others who helped me understand the stickier bits of emulation
//!
//! Also, a huge shout out to [OneLoneCoder](https://raw.githubusercontent.com/OneLoneCoder/) for
//! his [NES](https://raw.githubusercontent.com/OneLoneCoder/olcNES) and
//! [olcPixelGameEngine](https://raw.githubusercontent.com/OneLoneCoder/olcPixelGameEngine) series
//! as those helped a ton in some recent refactorings.
//!
//! [rust]: https://www.rust-lang.org/tools/install
//! [sdl2]: https://www.libsdl.org/
//! [wasm]: https://webassembly.org/

use pix_engine::prelude::*;
use pretty_env_logger as _;
use std::{borrow::Cow, fmt, result};
use structopt as _;

pub mod apu;
pub mod bus;
pub mod cartridge;
#[macro_use]
pub mod common;
pub mod control_deck;
pub mod cpu;
pub mod filter;
pub mod input;
pub mod mapper;
pub mod memory;
pub mod nes;
pub mod ppu;
pub mod serialization;

pub type NesResult<T> = result::Result<T, NesErr>;

pub struct NesErr {
    description: String,
}

impl NesErr {
    fn new<D: ToString>(desc: D) -> Self {
        Self {
            description: desc.to_string(),
        }
    }
    fn err<T, D: ToString>(desc: D) -> NesResult<T> {
        Err(Self {
            description: desc.to_string(),
        })
    }
}

#[macro_export]
macro_rules! nes_err {
    ($($arg:tt)*) => {
        crate::NesErr::err(&format!($($arg)*))
    };
}
#[macro_export]
macro_rules! map_nes_err {
    ($($arg:tt)*) => {
        crate::NesErr::new(&format!($($arg)*))
    };
}

impl fmt::Display for NesErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description)
    }
}

impl fmt::Debug for NesErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ err: {}, file: {}, line: {} }}",
            self.description,
            file!(),
            line!()
        )
    }
}

impl std::error::Error for NesErr {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl From<std::io::Error> for NesErr {
    fn from(err: std::io::Error) -> Self {
        Self {
            description: err.to_string(),
        }
    }
}

impl From<std::string::FromUtf8Error> for NesErr {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self {
            description: err.to_string(),
        }
    }
}

impl From<NesErr> for PixError {
    fn from(err: NesErr) -> Self {
        Self::Other(Cow::from(err.to_string()))
    }
}

impl From<RendererError> for NesErr {
    fn from(err: RendererError) -> Self {
        Self::new(&err.to_string())
    }
}

impl From<StateError> for NesErr {
    fn from(err: StateError) -> Self {
        Self::new(&err.to_string())
    }
}

impl From<PixError> for NesErr {
    fn from(err: PixError) -> Self {
        Self::new(&err.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
impl From<NesErr> for JsValue {
    fn from(err: NesErr) -> Self {
        JsValue::from_str(&err.to_string())
    }
}
