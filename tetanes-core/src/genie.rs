//! Game Genie code parsing.

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::OnceLock};
use thiserror::Error;

static GENIE_MAP: OnceLock<HashMap<char, u8>> = OnceLock::new();

/// The Game Genie alphabet, indexed by the nibble it stands for - the inverse of [`GENIE_MAP`].
const GENIE_LETTERS: [u8; 16] = *b"APZLGITYEOXUKSVN";

/// A `Result` from parsing a Game Genie code.
pub type Result<T> = std::result::Result<T, Error>;

/// An invalid Game Genie code.
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

    /// What was wrong with the code.
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }
}

/// Why a Game Genie code was rejected.
#[derive(Error, Debug, Copy, Clone)]
#[must_use]
pub enum ErrorKind {
    /// A code is 6 or 8 characters; this one was neither.
    #[error("length must be 6 or 8 characters. found `{0}`")]
    InvalidLength(usize),
    /// A character outside the Game Genie alphabet.
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
    /// Creates a `GenieCode` from a 6- or 8-letter Game Genie code.
    ///
    /// Codes patch reads from PRG-ROM: a 6-letter code replaces whatever is at its address, an
    /// 8-letter code only does so when the existing value matches its compare byte. Add one to a
    /// running console with
    /// [`ControlDeck::add_genie_code`](crate::control_deck::ControlDeck::add_genie_code).
    ///
    /// ```
    /// use tetanes_core::genie::GenieCode;
    ///
    /// // Infinite lives in Super Mario Bros.
    /// let code = GenieCode::new("SXIOPO".to_string())?;
    /// assert_eq!(code.code(), "SXIOPO");
    ///
    /// // Letters outside the Game Genie alphabet, or a wrong length, are rejected.
    /// assert!(GenieCode::new("NOTACODE".to_string()).is_err());
    /// # Ok::<(), tetanes_core::genie::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// If the code is not 6 or 8 characters, or contains a letter outside the Game Genie alphabet,
    /// then an error is returned.
    pub fn new(code: String) -> Result<Self> {
        let hex = Self::parse(&code)?;
        Ok(Self::from_raw(code, &hex))
    }

    /// Creates a new `GenieCode` instance from raw hex values. `GenieCode` may not be valid if
    /// `hex` is not the correct length. Use `GenieCode::parse` to validate the code.
    pub fn from_raw(code: String, hex: &[u8]) -> Self {
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

    /// Builds the code that patches `addr` to `data`, or only when the byte already there is
    /// `compare`.
    ///
    /// `None` below `$8000`: the Game Genie's address field is 15 bits over a forced `0x8000`
    /// base, so a RAM address has no code. Use [`Patch`](crate::patch::Patch) directly for those.
    ///
    /// The letters are canonical rather than unique. A 6-letter code is 24 bits carrying a 15-bit
    /// address and an 8-bit value, so one bit is spare - encoding a code that set it gives back a
    /// code that means the same thing and reads differently.
    ///
    /// ```
    /// use tetanes_core::genie::GenieCode;
    ///
    /// // Infinite lives in Super Mario Bros., the other way round.
    /// let code = GenieCode::new("SXIOPO".to_string())?;
    /// assert_eq!(GenieCode::encode(code.addr(), code.read(0), None).as_ref(), Some(&code));
    /// assert!(GenieCode::encode(0x0300, 0xFF, None).is_none(), "RAM has no code");
    /// # Ok::<(), tetanes_core::genie::Error>(())
    /// ```
    #[must_use]
    pub fn encode(addr: u16, data: u8, compare: Option<u8>) -> Option<Self> {
        let a = addr.checked_sub(0x8000)?;
        let (a, d) = (usize::from(a), usize::from(data));
        // The inverse of the permutation `from_raw` reads, nibble by nibble.
        let mut hex = vec![
            (d & 7) | ((d >> 4) & 8),
            ((d >> 4) & 7) | ((a >> 4) & 8),
            (a >> 4) & 7,
            ((a >> 12) & 7) | (a & 8),
            (a & 7) | ((a >> 8) & 8),
            ((a >> 8) & 7),
        ];
        if let Some(compare) = compare {
            let c = usize::from(compare);
            // An 8-letter code moves data's bit 3 to the last nibble to make room for `compare`.
            hex[5] |= c & 8;
            hex.push((c & 7) | ((c >> 4) & 8));
            hex.push(((c >> 4) & 7) | (d & 8));
        } else {
            hex[5] |= d & 8;
        }
        let code = hex
            .iter()
            .map(|&nibble| char::from(GENIE_LETTERS[nibble]))
            .collect::<String>();
        let hex = hex
            .into_iter()
            .map(|nibble| nibble as u8)
            .collect::<Vec<_>>();
        Some(Self::from_raw(code, &hex))
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

    /// Decodes a code's characters into their nibble values.
    ///
    /// # Errors
    ///
    /// If the code is the wrong length or holds a character outside the alphabet.
    pub fn parse(code: &str) -> Result<Box<[u8]>> {
        if code.len() != 6 && code.len() != 8 {
            return Err(Error::new(code, ErrorKind::InvalidLength(code.len())));
        }
        let mut hex = Vec::with_capacity(code.len());
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
        Ok(hex.into())
    }

    /// The code as entered.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // false positive on non-const deref coercion
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The CPU address the code patches.
    #[must_use]
    pub const fn addr(&self) -> u16 {
        self.addr
    }

    /// The value the code substitutes.
    #[must_use]
    pub const fn data(&self) -> u8 {
        self.data
    }

    /// The byte the code requires to be there already, if it is an 8-letter code.
    #[must_use]
    pub const fn compare(&self) -> Option<u8> {
        self.compare
    }

    /// Applies the code to a value read from that address, honouring the compare byte if the
    /// code has one.
    #[must_use]
    pub const fn read(&self, val: u8) -> u8 {
        if let Some(compare) = self.compare {
            if val == compare { self.data } else { val }
        } else {
            self.data
        }
    }
}

impl std::fmt::Display for GenieCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alphabet and the map are two spellings of one table, and only the map is exercised by
    /// decoding.
    #[test]
    fn the_alphabet_is_the_inverse_of_the_map() {
        let map = GENIE_MAP.get_or_init(GenieCode::generate_genie_map);
        assert_eq!(map.len(), GENIE_LETTERS.len());
        for (nibble, &letter) in GENIE_LETTERS.iter().enumerate() {
            assert_eq!(map[&char::from(letter)], nibble as u8, "letter {letter:?}");
        }
    }

    /// `encode` is `from_raw`'s bit permutation run backwards, and a permutation is only right if
    /// it round-trips - so drive every address bit, both code lengths, and some real codes.
    ///
    /// What round-trips is the *meaning*, not the letters: a 6-letter code is 24 bits carrying a
    /// 15-bit address and an 8-bit value, so one bit is spare, and this format leaves it in the
    /// third letter. `ZEXPYGLA` and `ZEZPYGLA` are the same code.
    #[test]
    fn a_code_survives_a_decode_and_encode_round_trip() {
        let mut codes = vec![
            // Super Mario Bros.: infinite lives, and start with a mushroom.
            "SXIOPO".to_string(),
            "AATOZA".to_string(),
            // Eight-letter codes, which carry a compare byte.
            "ZEXPYGLA".to_string(),
            "GXXZZLVI".to_string(),
        ];
        // One code per address bit, so a permutation that transposes two of them cannot pass.
        for bit in 0..15 {
            let code = GenieCode::encode(0x8000 | (1 << bit), 0x5A, None).expect("in ROM space");
            codes.push(code.code().to_string());
        }

        for code in codes {
            let decoded = GenieCode::new(code.clone()).expect("valid code");
            let encoded = GenieCode::encode(decoded.addr(), decoded.data(), decoded.compare())
                .expect("addr is in ROM space");
            assert_eq!(encoded.addr(), decoded.addr(), "{code} address");
            assert_eq!(encoded.data(), decoded.data(), "{code} data");
            assert_eq!(encoded.compare(), decoded.compare(), "{code} compare");
            assert_eq!(encoded.code().len(), code.len(), "{code} keeps its length");
            // Re-decoding the emitted letters must land in the same place, which the spare bit
            // cannot affect.
            let round_tripped = GenieCode::new(encoded.code().to_string()).expect("valid code");
            assert_eq!(round_tripped, encoded, "{code} re-decodes to itself");
        }
    }

    /// The spare bit is why re-encoding a real code can change a letter without changing what the
    /// code does. Anything that compares codes as strings needs to know.
    #[test]
    fn the_third_letters_high_bit_carries_nothing() {
        let with = GenieCode::new("ZEXPYGLA".to_string()).expect("valid code");
        let without = GenieCode::new("ZEZPYGLA".to_string()).expect("valid code");
        assert_eq!(
            (with.addr(), with.data(), with.compare()),
            (without.addr(), without.data(), without.compare()),
        );
    }

    /// Every data and compare bit has to survive too, not just the address.
    #[test]
    fn encoding_round_trips_every_data_and_compare_byte() {
        for data in 0..=u8::MAX {
            let code = GenieCode::encode(0x9ABC, data, None).expect("in ROM space");
            let decoded = GenieCode::new(code.code().to_string()).expect("valid code");
            assert_eq!(decoded.addr(), 0x9ABC, "data {data:#04X}");
            assert_eq!(decoded.data(), data);
            assert_eq!(decoded.compare(), None);

            let code = GenieCode::encode(0x9ABC, 0x5A, Some(data)).expect("in ROM space");
            let decoded = GenieCode::new(code.code().to_string()).expect("valid code");
            assert_eq!(decoded.addr(), 0x9ABC, "compare {data:#04X}");
            assert_eq!(decoded.data(), 0x5A);
            assert_eq!(decoded.compare(), Some(data));
        }
    }

    #[test]
    fn an_address_below_rom_has_no_code() {
        assert!(GenieCode::encode(0x7FFF, 0xAA, None).is_none());
        assert!(GenieCode::encode(0x0000, 0xAA, None).is_none());
    }
}
