pub trait Loggable {
    fn set_log_level(&mut self, level: LogLevel);
    fn log_level(&self) -> LogLevel;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
use LogLevel::*;

impl LogLevel {
    pub fn increase(level: LogLevel) -> Self {
        match level {
            Error => Warn,
            Warn => Info,
            Info => Debug,
            Debug => Trace,
            Trace => Off,
            Off => Error,
        }
    }
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Error
    }
}

#[macro_export]
macro_rules! error(
    ($self:ident, $($arg:tt)*) => {
        if $self.log_level() >= LogLevel::Error {
            println!($($arg)*)
        }
    };
);
#[macro_export]
macro_rules! warn(
    ($self:ident, $($arg:tt)*) => {
        if $self.log_level() >= LogLevel::Warn {
            println!($($arg)*)
        }
    };
);
#[macro_export]
macro_rules! info(
    ($self:ident, $($arg:tt)*) => {
        if $self.log_level() >= LogLevel::Info {
            println!($($arg)*)
        }
    };
);
#[macro_export]
macro_rules! debug(
    ($self:ident, $($arg:tt)*) => {
        if $self.log_level() >= LogLevel::Debug {
            println!($($arg)*)
        }
    };
);
#[macro_export]
macro_rules! trace(
    ($self:ident, $($arg:tt)*) => {
        if $self.log_level() >= LogLevel::Trace {
            println!($($arg)*)
        }
    };
);
