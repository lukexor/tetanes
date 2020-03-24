use crate::nes::menu::Message;
use pix_engine::sprite::Sprite;

#[derive(Clone)]
pub(super) struct Window {
    pub(super) id: u32,
    pub(super) title: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) textures: Vec<Sprite>,
    pub(super) messages: Vec<Message>,
}

impl Window {
    pub(super) fn new(id: u32, title: &str, width: u32, height: u32) -> Self {
        Self {
            id,
            title: title.to_owned(),
            width,
            height,
            textures: vec![Sprite::new(width, height)],
            messages: Vec::new(),
        }
    }
}
