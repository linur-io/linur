// SPDX-License-Identifier: GPL-3.0-only
//! Syscall dispatch.
//!
//! `table` owns the architecture-specific number→handler mapping; the handler
//! modules below are the shared semantics.

mod fs;
mod process;
mod sync;
mod table;
mod thread;

pub use table::dispatch;
