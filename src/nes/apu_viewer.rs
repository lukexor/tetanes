use crate::nes::Nes;
use pix_engine::prelude::*;

#[derive(Debug)]
pub(crate) struct ApuViewer {
    window_id: WindowId,
}

impl ApuViewer {
    const fn new(window_id: WindowId) -> Self {
        Self { window_id }
    }

    pub(crate) const fn window_id(&self) -> WindowId {
        self.window_id
    }
}

impl Nes {
    pub(crate) fn toggle_apu_viewer(&mut self, s: &mut PixState) -> PixResult<()> {
        match self.apu_viewer {
            None => {
                let w = s.width()?;
                let h = s.height()?;
                let window_id = s
                    .window()
                    .with_dimensions(w, h)
                    .with_title("APU Viewer")
                    .position(10, 10)
                    .build()?;
                self.apu_viewer = Some(ApuViewer::new(window_id));
            }
            Some(ref viewer) => {
                s.close_window(viewer.window_id())?;
                self.apu_viewer = None;
            }
        }
        Ok(())
    }
}
