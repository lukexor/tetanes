//! Diagnostics, which have to go through the frontend.
//!
//! Most frontends swallow a core's stderr, so anything written there is invisible where it matters.
//! `GET_LOG_INTERFACE` hands back a `printf`-style callback that lands in the frontend's own log.

use crate::sys::{self, retro_log_callback, retro_log_printf_t};
use std::{
    cell::Cell,
    ffi::{CString, c_int, c_void},
};

thread_local! {
    /// The frontend's logger, once `GET_LOG_INTERFACE` has answered.
    ///
    /// A frontend need not provide one, in which case diagnostics are dropped - there is nowhere
    /// else for them to go.
    static LOGGER: Cell<Option<retro_log_printf_t>> = const { Cell::new(None) };
}

/// Asks the frontend for its logger. Called once, from `retro_set_environment`.
///
/// # Safety
///
/// `environment` must be the callback the frontend handed over.
pub unsafe fn init(environment: sys::retro_environment_t) {
    let mut callback = retro_log_callback { log: None };
    // SAFETY: the frontend either fills `callback.log` and returns true, or leaves it alone.
    let ok = unsafe {
        environment(
            sys::RETRO_ENVIRONMENT_GET_LOG_INTERFACE,
            std::ptr::from_mut(&mut callback).cast::<c_void>(),
        )
    };
    if ok {
        LOGGER.set(callback.log);
    }
}

/// Forgets the frontend's logger. Called from `retro_deinit`.
pub fn deinit() {
    LOGGER.set(None);
}

fn write(level: c_int, message: &str) {
    let Some(log) = LOGGER.get() else {
        return;
    };
    // The message becomes a `printf` argument rather than its format string, so a `%` a ROM name
    // happens to contain cannot make the frontend read arguments that were never passed.
    let Ok(message) = CString::new(message) else {
        return;
    };
    // SAFETY: `log` came from the frontend, and both strings are NUL-terminated and outlive the
    // call. `%s` consumes exactly the one argument supplied.
    unsafe { log(level, c"[TetaNES] %s\n".as_ptr(), message.as_ptr()) };
}

/// Logs a message the user is expected to act on.
pub fn error(message: &str) {
    write(sys::RETRO_LOG_ERROR, message);
}

/// Logs a message about the core operating normally.
pub fn info(message: &str) {
    write(sys::RETRO_LOG_INFO, message);
}
