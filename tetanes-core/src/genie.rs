use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::OnceLock};
use thiserror::Error;

static GENIE_MAP: OnceLock<HashMap<char, u8>> = OnceLock::new();

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[error("invalid genie code {code:?}. {kind}")]
pub struct Error {
    code: String,
    kind: ErrorKind,
}

impl Error {
    fn new(code: impl Into<String>, kind: ErrorKind) -> Self {
        Self {
            code: code.into(),
            kind,
        }
    }

    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }
}

#[derive(Error, Debug, Copy, Clone)]
#[must_use]
pub enum ErrorKind {
    #[error("length must be 6 or 8 characters. found `{0}`")]
    InvalidLength(usize),
    #[error("invalid character: `{0}`")]
    InvalidCharacter(char),
}

/// Game Genie Code
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenieCode {
    code: String,
    addr: u16,
    data: u8,
    compare: Option<u8>,
}

impl GenieCode {
    /// Creates a new `GenieCode` instance.
    ///
    /// # Errors
    ///
    /// This function will return an error if the given code is not the correct format.
    pub fn new(code: String) -> Result<Self> {
        let hex = Self::parse(&code)?;
        Ok(Self::from_raw(code, hex))
    }

    /// Creates a new `GenieCode` instance from raw hex values. `GenieCode` may not be valid if
    /// `hex` is not the correct length. Use `GenieCode::parse` to validate the code.
    pub fn from_raw(code: String, hex: Vec<u8>) -> Self {
        let addr = 0x8000
            + (((u16::from(hex[3]) & 7) << 12)
                | ((u16::from(hex[5]) & 7) << 8)
                | ((u16::from(hex[4]) & 8) << 8)
                | ((u16::from(hex[2]) & 7) << 4)
                | ((u16::from(hex[1]) & 8) << 4)
                | (u16::from(hex[4]) & 7)
                | (u16::from(hex[3]) & 8));
        let data = if hex.len() == 6 {
            ((hex[1] & 7) << 4) | ((hex[0] & 8) << 4) | (hex[0] & 7) | (hex[5] & 8)
        } else {
            ((hex[1] & 7) << 4) | ((hex[0] & 8) << 4) | (hex[0] & 7) | (hex[7] & 8)
        };
        let compare = if hex.len() == 8 {
            Some(((hex[7] & 7) << 4) | ((hex[6] & 8) << 4) | (hex[6] & 7) | (hex[5] & 8))
        } else {
            None
        };
        Self {
            code: code.to_ascii_uppercase(),
            addr,
            data,
            compare,
        }
    }

    fn generate_genie_map() -> HashMap<char, u8> {
        // Game genie maps these letters to binary representations as a form of code obfuscation
        HashMap::from([
            ('A', 0x0),
            ('P', 0x1),
            ('Z', 0x2),
            ('L', 0x3),
            ('G', 0x4),
            ('I', 0x5),
            ('T', 0x6),
            ('Y', 0x7),
            ('E', 0x8),
            ('O', 0x9),
            ('X', 0xA),
            ('U', 0xB),
            ('K', 0xC),
            ('S', 0xD),
            ('V', 0xE),
            ('N', 0xF),
        ])
    }

    pub fn parse(code: &str) -> Result<Vec<u8>> {
        if code.len() != 6 && code.len() != 8 {
            return Err(Error::new(code, ErrorKind::InvalidLength(code.len())));
        }
        let mut hex: Vec<u8> = Vec::with_capacity(code.len());
        for s in code.chars() {
            if let Some(h) = GENIE_MAP
                .get_or_init(Self::generate_genie_map)
                .get(&s.to_ascii_uppercase())
            {
                hex.push(*h);
            } else {
                return Err(Error::new(code, ErrorKind::InvalidCharacter(s)));
            }
        }
        Ok(hex)
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub const fn addr(&self) -> u16 {
        self.addr
    }

    #[must_use]
    pub const fn read(&self, val: u8) -> u8 {
        if let Some(compare) = self.compare {
            if val == compare {
                self.data
            } else {
                val
            }
        } else {
            self.data
        }
    }
}

impl std::fmt::Display for GenieCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.code)
    }
}
