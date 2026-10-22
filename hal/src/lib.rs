// SPDX-License-Identifier: GPL-3.0-only
//! The architecture seam: the two primitives both kernels need before anything
//! else exists. Implementations live next to the hardware they wrap.
#![no_std]
#![deny(missing_docs)]

/// A byte sink for kernel messages.
pub trait Console {
    /// Write one byte. No encoding is assumed: the caller may forward any byte.
    fn put_byte(&self, byte: u8);
}

/// The machine's stop primitive.
pub trait Exit {
    /// Stop the machine. `0` means success; any other value means failure.
    /// A correct implementation does not return.
    fn exit(&self, code: i32) -> !;
}
