//! Diagnostics, which have to reach the frontend.
//!
//! Most frontends swallow a core's stderr, so anything written there is invisible where it matters.
//! `GET_LOG_INTERFACE` hands back a `printf`-style callback that lands in the frontend's own log.
//! Anything that cannot get there is written to stderr as well, which is all that is left when a
//! frontend provides no logger - and is what a core run from a terminal shows.

use crate::sys::{self, retro_log_callback, retro_log_printf_t};
use std::{
    ffi::{CString, c_int, c_void},
    panic,
    sync::{
        Once,
        atomic::{AtomicUsize, Ordering},
    },
};

/// The frontend's logger, or zero.
///
/// A `static` rather than a `thread_local!`: a frontend may hand this over on one thread and run
/// the core on another, and a per-thread logger would then drop every diagnostic the emulation
/// produced - exactly when they are wanted.
static LOGGER: AtomicUsize = AtomicUsize::new(0);

static PANIC_HOOK: Once = Once::new();

fn logger() -> Option<retro_log_printf_t> {
    let addr = LOGGER.load(Ordering::Relaxed);
    // SAFETY: only ever stored from a `retro_log_printf_t` the frontend supplied, and a function
    // pointer is the width of a `usize`.
    (addr != 0).then(|| unsafe { std::mem::transmute::<usize, retro_log_printf_t>(addr) })
}

/// Asks the frontend for its logger, and routes panics to it.
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
    if ok && let Some(log) = callback.log {
        LOGGER.store(log as usize, Ordering::Relaxed);
    }

    // Without this a panic is reported as the fact that one happened and nothing else, which is
    // not enough to act on - and the default hook writes to stderr, where a frontend cannot see it.
    PANIC_HOOK.call_once(|| {
        let default = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let location = info
                .location()
                .map_or_else(|| "unknown location".to_string(), ToString::to_string);
            error(&format!(
                "panicked at {location}: {}",
                payload(info.payload())
            ));
            default(info);
        }));
    });
}

/// Forgets the frontend's logger. Called from `retro_deinit`.
///
/// The panic hook stays: a library cannot un-install one without stepping on whatever replaced it,
/// and it is harmless once nothing is logging.
pub fn deinit() {
    LOGGER.store(0, Ordering::Relaxed);
}

/// The message out of a panic payload, for the two shapes `panic!` produces.
pub fn payload(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("(no message)")
}

fn write(level: c_int, message: &str) {
    let Some(log) = logger() else {
        // Nowhere else for it to go, and a core run from a terminal shows this.
        eprintln!("[TetaNES] {message}");
        return;
    };
    // The message becomes a `printf` argument rather than its format string, so a `%` in a ROM
    // name cannot make the frontend read arguments that were never passed.
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
