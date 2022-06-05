use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Menu {
    Config,
    Keybind,
    LoadRom,
    About,
}

impl AsRef<str> for Menu {
    fn as_ref(&self) -> &str {
        match self {
            Self::Config => "Configuration",
            Self::Keybind => "Keybindings",
            Self::LoadRom => "Load ROM",
            Self::About => "About",
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum Player {
    One,
    Two,
    Three,
    Four,
}

impl Player {
    #[inline]
    #[must_use]
    pub(crate) const fn as_slice() -> &'static [Self] {
        &[Self::One, Self::Two, Self::Three, Self::Four]
    }
}

impl AsRef<str> for Player {
    fn as_ref(&self) -> &str {
        match self {
            Self::One => "Player One",
            Self::Two => "Player Two",
            Self::Three => "Player Three",
            Self::Four => "Player Four",
        }
    }
}

impl From<usize> for Player {
    fn from(value: usize) -> Self {
        match value {
            1 => Self::Two,
            2 => Self::Three,
            3 => Self::Four,
            _ => Self::One,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum SampleRate {
    S32,
    S44,
    S48,
    S96,
}

impl SampleRate {
    #[inline]
    #[must_use]
    pub(crate) const fn as_slice() -> &'static [Self] {
        &[Self::S32, Self::S44, Self::S48, Self::S96]
    }

    #[inline]
    #[must_use]
    pub(crate) const fn as_f32(self) -> f32 {
        match self {
            Self::S32 => 32000.0,
            Self::S44 => 44100.0,
            Self::S48 => 48000.0,
            Self::S96 => 96000.0,
        }
    }
}

impl AsRef<str> for SampleRate {
    fn as_ref(&self) -> &str {
        match self {
            Self::S32 => "32 kHz",
            Self::S44 => "44.1 kHz",
            Self::S48 => "48 kHz",
            Self::S96 => "96 kHz",
        }
    }
}

impl From<usize> for SampleRate {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::S32,
            1 => Self::S44,
            3 => Self::S96,
            _ => Self::S48,
        }
    }
}

impl From<f32> for SampleRate {
    fn from(value: f32) -> Self {
        match value as i32 {
            32000 => Self::S32,
            44100 => Self::S44,
            96000 => Self::S96,
            _ => Self::S48,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum EmuSpeed {
    S25,
    S50,
    S75,
    S100,
    S125,
    S150,
    S175,
    S200,
}

impl EmuSpeed {
    #[inline]
    #[must_use]
    pub(crate) const fn as_slice() -> &'static [Self] {
        &[
            Self::S25,
            Self::S50,
            Self::S75,
            Self::S100,
            Self::S125,
            Self::S150,
            Self::S175,
            Self::S200,
        ]
    }

    #[inline]
    #[must_use]
    pub(crate) const fn as_f32(self) -> f32 {
        match self {
            Self::S25 => 0.25,
            Self::S50 => 0.50,
            Self::S75 => 0.75,
            Self::S100 => 1.0,
            Self::S125 => 1.25,
            Self::S150 => 1.5,
            Self::S175 => 1.75,
            Self::S200 => 2.0,
        }
    }
}

impl AsRef<str> for EmuSpeed {
    fn as_ref(&self) -> &str {
        match self {
            Self::S25 => "25%",
            Self::S50 => "50%",
            Self::S75 => "75%",
            Self::S100 => "100%",
            Self::S125 => "125%",
            Self::S150 => "150%",
            Self::S175 => "175%",
            Self::S200 => "200%",
        }
    }
}

impl From<usize> for EmuSpeed {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::S25,
            1 => Self::S50,
            2 => Self::S75,
            4 => Self::S125,
            5 => Self::S150,
            6 => Self::S175,
            7 => Self::S200,
            _ => Self::S100,
        }
    }
}

impl From<f32> for EmuSpeed {
    fn from(value: f32) -> Self {
        Self::from(((4.0 * value) as usize).saturating_sub(1))
    }
}
