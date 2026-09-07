use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use thiserror::Error;

#[derive(Error, Debug)]
#[must_use]
#[error("failed to parse `VideoFilter`")]
pub struct ParseShaderError;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub enum Shader {
    Default,
    #[default]
    CrtEasymode,
}

impl Shader {
    pub const fn as_slice() -> &'static [Self] {
        &[Self::Default, Self::CrtEasymode]
    }

    /// Shader module source for the pipeline that draws NES textures, or `None` when they go
    /// through the gui pipeline unfiltered.
    pub fn source(self) -> Option<Cow<'static, str>> {
        match self {
            Self::Default => None,
            Self::CrtEasymode => Some(module_source(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/shaders/crt-easymode.wgsl"
            )))),
        }
    }
}

impl AsRef<str> for Shader {
    fn as_ref(&self) -> &str {
        match self {
            Self::Default => "Default",
            Self::CrtEasymode => "CRT Easymode",
        }
    }
}

impl TryFrom<usize> for Shader {
    type Error = ParseShaderError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Default,
            1 => Self::CrtEasymode,
            _ => return Err(ParseShaderError),
        })
    }
}

/// Shader module source for the gui pipeline, which draws every egui mesh.
pub fn gui_source() -> Cow<'static, str> {
    module_source(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/shaders/gui.wgsl"
    )))
}

/// The vertex stage, bind groups and color helpers every pipeline shares.
const COMMON: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/common.wgsl"));

/// Join [`COMMON`] to a fragment stage to make a complete shader module.
fn module_source(fragment: &str) -> Cow<'static, str> {
    Cow::Owned(format!("{COMMON}\n{fragment}"))
}
