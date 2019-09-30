use crate::input::Key;

pub enum PixEvent {
    None,
    Quit,
    AppTerminating,
    KeyPress(Key, bool),
}
