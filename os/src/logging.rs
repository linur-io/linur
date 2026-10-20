// SPDX-License-Identifier: GPL-3.0-only
//! Logging: five levels, no third-party crate.
//!
//! The ceiling is fixed at build time by the `LOG` environment variable
//! (`ERROR`, `WARN`, `INFO`, `DEBUG`, `TRACE`; `INFO` when unset), so
//! records above it are discarded before their arguments are formatted.

use crate::console;
use core::fmt;

/// Log severity, most severe first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// A failure the kernel cannot recover from.
    Error,
    /// Something unexpected that the kernel handled.
    Warn,
    /// Normal boot and lifecycle messages.
    Info,
    /// Details useful while developing a subsystem.
    Debug,
    /// Everything, including per-event traces.
    Trace,
}

impl Level {
    /// Uppercase name, padded to five columns for the log prefix.
    fn name(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => " WARN",
            Level::Info => " INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }
}

/// The highest level that is emitted, taken from the `LOG` build-time variable.
fn max_level() -> Level {
    match option_env!("LOG") {
        Some("ERROR") => Level::Error,
        Some("WARN") => Level::Warn,
        Some("DEBUG") => Level::Debug,
        Some("TRACE") => Level::Trace,
        _ => Level::Info,
    }
}

/// Print one record as `[LEVEL] message` unless it is above the ceiling.
pub fn log(level: Level, args: fmt::Arguments) {
    if level <= max_level() {
        console::print(format_args!("[{}] {}\n", level.name(), args));
    }
}

#[macro_export]
macro_rules! error {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::logging::log(
            $crate::logging::Level::Error,
            format_args!($fmt $(, $($arg)+)?),
        )
    };
}

#[macro_export]
macro_rules! warn {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::logging::log(
            $crate::logging::Level::Warn,
            format_args!($fmt $(, $($arg)+)?),
        )
    };
}

#[macro_export]
macro_rules! info {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::logging::log(
            $crate::logging::Level::Info,
            format_args!($fmt $(, $($arg)+)?),
        )
    };
}

#[macro_export]
macro_rules! debug {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::logging::log(
            $crate::logging::Level::Debug,
            format_args!($fmt $(, $($arg)+)?),
        )
    };
}

#[macro_export]
macro_rules! trace {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::logging::log(
            $crate::logging::Level::Trace,
            format_args!($fmt $(, $($arg)+)?),
        )
    };
}
