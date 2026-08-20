//! What the Debugger remembers about a ROM between sessions.
//!
//! One file per ROM beside its save states and battery RAM, holding what execution has shown its
//! bytes to be, the breakpoints set on them, and the names given to them. All of it indexes the
//! cart's memory, so a file recorded against a different cart is refused rather than applied.
//!
//! The two halves have different owners. The window owns [`Marks`] and the emulation thread owns
//! the [`CodeMap`], so the marks travel to the emulation thread to be written and come back when
//! a ROM loads. Only the small half crosses the channel.

use crate::nes::{config::Config, renderer::gui::debugger::Breakpoint};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tetanes_core::{debug::CodeMap, fs, memory::Memory};
use tracing::{error, info};

/// The version this format is at.
///
/// Independent of the save-state version, the way the game database's is: a change to how a
/// console is serialized says nothing about whether a debug file is still readable.
const DEBUG_VERSION: &str = "1";

/// What the Debugger's window keeps for a ROM.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[must_use]
pub struct Marks {
    /// The breakpoints the window lists, disarmed ones included.
    pub breakpoints: Vec<Breakpoint>,
    /// A name per cart offset, keyed the way a [`CodeMap`] is so a name follows the bytes it
    /// belongs to through a bank switch.
    pub labels: HashMap<u32, String>,
}

impl Marks {
    /// Whether there is nothing here to write.
    pub fn is_empty(&self) -> bool {
        self.breakpoints.is_empty() && self.labels.is_empty()
    }
}

/// One ROM's debugging session, as it is written to disk.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Session {
    /// What execution has shown the cart's bytes to be, or `None` where nothing has been recorded.
    pub code_map: Option<CodeMap>,
    /// What the window set on them.
    pub marks: Marks,
}

impl Session {
    /// Directory the files live in, under the data directory beside `save` and `sram`.
    const DIR: &'static str = "debug";
    /// Extension the files take.
    const EXTENSION: &'static str = "dbg";

    /// Where the file for the ROM named `name` lives.
    pub fn path(name: &str) -> PathBuf {
        Config::default_data_dir()
            .join(Self::DIR)
            .join(name)
            .with_extension(Self::EXTENSION)
    }

    /// Read what was saved for `name`, or an empty session where there is nothing to read.
    ///
    /// A file that will not parse is reported and skipped. Losing the marks a session made is
    /// worth saying out loud, and is not a reason to refuse the ROM.
    pub fn load(name: &str) -> Self {
        Self::load_path(Self::path(name))
    }

    /// [`Session::load`] from an explicit path.
    pub fn load_path(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if !fs::exists(path) {
            return Self::default();
        }
        match fs::load_version_path(path, DEBUG_VERSION) {
            Ok(session) => {
                info!("loaded debug session: {path:?}");
                session
            }
            Err(err) => {
                error!("failed to load debug session {path:?}: {err:?}");
                Self::default()
            }
        }
    }

    /// Write this session for `name`.
    ///
    /// # Errors
    ///
    /// If the file cannot be written.
    pub fn save(&self, name: &str) -> fs::Result<()> {
        self.save_path(Self::path(name))
    }

    /// [`Session::save`] to an explicit path.
    ///
    /// # Errors
    ///
    /// If the file cannot be written.
    pub fn save_path(&self, path: impl AsRef<Path>) -> fs::Result<()> {
        // A session with nothing in it writes nothing, so opening the Debugger on a ROM and
        // closing it again leaves no file behind.
        if self.is_empty() {
            return Ok(());
        }
        fs::save_version_path(path, self, DEBUG_VERSION)
    }

    /// Whether there is nothing here worth writing.
    pub fn is_empty(&self) -> bool {
        self.code_map.is_none() && self.marks.is_empty()
    }

    /// Drop what does not describe `memory`.
    ///
    /// A code map's offsets and a breakpoint's only mean anything against the arena the loaded
    /// cart produces, so a file read for one game cannot be applied to another that happens to
    /// share its name.
    pub fn accept(&mut self, memory: &Memory) {
        if !self
            .code_map
            .as_ref()
            .is_some_and(|code_map| code_map.covers(memory))
        {
            self.code_map = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetanes_core::{
        cart::Cart,
        debug::{Access, ByteKind},
        mapper::Nrom,
    };

    fn cart() -> Cart {
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&mut cart).expect("valid mapper");
        cart
    }

    fn marked(cart: &Cart) -> Session {
        let mut code_map = CodeMap::new(cart.memory.len(), cart.memory.rom_crc32());
        code_map.mark(0x10, ByteKind::CODE);
        Session {
            code_map: Some(code_map),
            marks: Marks {
                breakpoints: vec![Breakpoint {
                    id: 7,
                    addr: 0x8123,
                    end: 0x8123,
                    offset: Some(0x1234),
                    access: Access::EXEC,
                    enabled: true,
                    breaks: true,
                    condition: "a == 0x10".to_string(),
                }],
                labels: HashMap::from([(0x1234, "reset".to_string())]),
            },
        }
    }

    /// What a session is worth keeping has to survive the file, or the next run of the Debugger
    /// starts over. The id is not part of that: it names a breakpoint for one run of the window.
    #[test]
    fn a_session_reads_back_what_it_wrote() {
        let dir = std::env::temp_dir().join("tetanes-debug-session-round-trip");
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("session.dbg");
        let cart = cart();
        let written = marked(&cart);
        written.save_path(&path).expect("writes");

        let read = Session::load_path(&path);
        let code_map = read.code_map.as_ref().expect("a map was written");
        assert!(code_map.covers(&cart.memory), "the map still fits the cart");
        assert_eq!(read.marks.labels, written.marks.labels);
        assert_eq!(read.marks.breakpoints.len(), 1);
        let breakpoint = &read.marks.breakpoints[0];
        assert_eq!(breakpoint.addr, 0x8123);
        assert_eq!(breakpoint.condition, "a == 0x10");
        assert!(breakpoint.enabled, "an armed breakpoint comes back armed");
        assert_eq!(breakpoint.id, 0, "the window hands ids out again");

        std::fs::remove_file(&path).expect("cleans up");
    }

    /// Offsets only address the cart they were recorded against, so a file that names another one
    /// cannot be applied to this one however it came to share its name.
    #[test]
    fn a_map_recorded_against_another_cart_is_refused() {
        let cart = cart();
        let mut session = marked(&cart);
        session.accept(&cart.memory);
        assert!(session.code_map.is_some(), "the cart it was recorded for");

        let mut other = Cart::empty_sized(0x8000, 0x2000);
        other.mapper = Nrom::load(&mut other).expect("valid mapper");
        let mut session = marked(&cart);
        session.accept(&other.memory);
        assert!(session.code_map.is_none(), "a cart of another size");
    }

    /// Nothing to keep writes no file, so opening the Debugger on a ROM and closing it again does
    /// not litter the data directory.
    #[test]
    fn an_empty_session_writes_no_file() {
        let path = std::env::temp_dir().join("tetanes-debug-session-empty.dbg");
        Session::default().save_path(&path).expect("writes nothing");
        assert!(!fs::exists(&path));
    }
}
