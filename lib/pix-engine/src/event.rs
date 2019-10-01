// Represents an input event
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum PixEvent {
    None,
    Quit,
    AppTerminating,
    KeyPress(Key, bool),
    MousePress(Mouse, i32, i32, bool),
    MouseWheel(i32),
    MouseMotion(i32, i32),
    Resized,
    Focus(bool),
    Background(bool),
}

/// Represents a user key/button input
#[derive(Debug, Copy, Clone)]
pub struct Input {
    pub pressed: bool,  // Set once during the frame in which it occurs
    pub released: bool, // Set once during the frame in which it occurs
    pub held: bool,     // Set for all frames between pressed and released
}

impl Input {
    pub(super) fn new() -> Self {
        Self {
            pressed: false,
            released: false,
            held: false,
        }
    }
}

/// Represents a mouse button
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Mouse {
    Left,
    Middle,
    Right,
    X1,
    X2,
    Unknown,
}

/// A non-exhaustive list of useful keys to detect
#[rustfmt::skip]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    Kp0, Kp1, Kp2, Kp3, Kp4, Kp5, Kp6, Kp7, Kp8, Kp9,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Left, Up, Down, Right,
    Tab, Insert, Delete, Home, End, PageUp, PageDown,
    Escape, Backspace, Return, KpEnter, Pause, ScrollLock,
    Plus, Minus, Period, Underscore, Equals,
    KpMultiply, KpDivide, KpPlus, KpMinus, KpPeriod,
    Backquote, Exclaim, At, Hash, Dollar, Percent,
    Caret, Ampersand, Asterisk, LeftParen, RightParen,
    LeftBracket, RightBracket, Backslash,
    CapsLock, Semicolon, Colon, Quotedbl, Quote,
    Less, Comma, Greater, Question, Slash,
    Shift, Space, Control, Alt, Meta,
    Unknown,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}
