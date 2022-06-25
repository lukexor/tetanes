use crate::NesResult;
use anyhow::anyhow;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

static GENIE_MAP: Lazy<HashMap<char, u8>> = Lazy::new(|| {
    // Game genie maps these letters to binary representations as a form of code obfuscation
    hashmap! {
        'A' => 0x0, 'P' => 0x1, 'Z' => 0x2, 'L' => 0x3, 'G' => 0x4, 'I' => 0x5, 'T' => 0x6,
        'Y' => 0x7, 'E' => 0x8, 'O' => 0x9, 'X' => 0xA, 'U' => 0xB, 'K' => 0xC, 'S' => 0xD,
        'V' => 0xE, 'N' => 0xF
    }
});

/// Game Genie Code
#[derive(Clone, Serialize, Deserialize)]
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
    pub fn new(code: String) -> NesResult<Self> {
        if code.len() != 6 && code.len() != 8 {
            return Err(anyhow!("invalid game genie code: {}", code));
        }
        let mut hex: Vec<u8> = Vec::with_capacity(code.len());
        for s in code.chars() {
            if let Some(h) = GENIE_MAP.get(&s) {
                hex.push(*h);
            } else {
                return Err(anyhow!("invalid game genie code: {}", code));
            }
        }
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
        Ok(Self {
            code,
            addr,
            data,
            compare,
        })
    }

    #[inline]
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[inline]
    #[must_use]
    pub fn addr(&self) -> u16 {
        self.addr
    }

    #[inline]
    #[must_use]
    pub fn matches(&self, val: u8) -> u8 {
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

impl std::fmt::Debug for GenieCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.code)
    }
}
