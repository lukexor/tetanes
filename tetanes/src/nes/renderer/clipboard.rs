#[must_use]
pub struct Clipboard {
    #[cfg(not(target_arch = "wasm32"))]
    inner: Option<arboard::Clipboard>,
    /// Fallback.
    text: String,
}

impl Default for Clipboard {
    #[allow(clippy::derivable_impls)]
    fn default() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            inner: arboard::Clipboard::new()
                .map_err(|err| tracing::warn!("failed to initialize clipboard: {err:?}"))
                .ok(),
            text: String::new(),
        }
    }
}

impl std::fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("Clipboard");
        #[cfg(not(target_arch = "wasm32"))]
        res.field("inner", &self.inner.as_ref().map(|_| "arboard"));
        res.field("text", &self.text).finish_non_exhaustive()
    }
}

impl Clipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&mut self) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(inner) = self.inner.as_mut() {
            return inner
                .get_text()
                .map_err(|err| tracing::warn!("clipboard paste error: {err:?}"))
                .ok();
        }

        Some(self.text.clone())
    }

    pub fn set(&mut self, text: impl Into<String>) {
        let text = text.into();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(inner) = self.inner.as_mut() {
            if let Err(err) = inner.set_text(text) {
                tracing::warn!("clipboard paste error: {err:?}");
            }
            return;
        }

        self.text = text
    }
}
