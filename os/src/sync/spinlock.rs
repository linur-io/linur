// SPDX-License-Identifier: GPL-3.0-only
//! A spinlock for state touched in syscall context (docs/block.md §6).
//!
//! Every trap runs with interrupts disabled in this kernel, so a lock taken in a
//! syscall cannot be preempted, and an interrupt handler must never take one of
//! these. M3.1's device layer revisits that when completions become IRQ-driven.

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// A spinlock: one atomic flag and the data it guards.
pub struct SpinMutex<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinMutex<T> {}

impl<T> SpinMutex<T> {
    /// Wrap `data` in an unlocked spinlock.
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Take the lock, spinning until it is free.
    pub fn lock(&self) -> SpinMutexGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }
        SpinMutexGuard { lock: self }
    }
}

/// The guard of a [`SpinMutex`]; dropping it releases the lock.
pub struct SpinMutexGuard<'a, T> {
    lock: &'a SpinMutex<T>,
}

impl<T> Deref for SpinMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}
