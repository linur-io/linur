// SPDX-License-Identifier: GPL-3.0-only
#[allow(unused)]
pub const USER_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_HEAP_SIZE: usize = 0x20_0000;
pub const PAGE_SIZE: usize = 0x1000;
pub const PAGE_SIZE_BITS: usize = 0xc;

pub const TRAMPOLINE: usize = usize::MAX - PAGE_SIZE + 1;
pub const TRAP_CONTEXT_BASE: usize = TRAMPOLINE - PAGE_SIZE;

/// Stride scheduler: numerator of a task's pass, `BIG_STRIDE / priority`.
pub const BIG_STRIDE: isize = 240;
/// Default scheduling weight of a task.
pub const DEFAULT_PRIORITY: isize = 16;
/// Smallest weight `set_priority` accepts.
pub const MIN_PRIORITY: isize = 2;
/// User virtual address where the `brk` heap starts.
pub const USER_HEAP_BASE: usize = 0x4000_0000;

pub use crate::board::{CLOCK_FREQ, MEMORY_END, MMIO};
