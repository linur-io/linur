// SPDX-License-Identifier: GPL-3.0-only
use crate::sbi::console_putchar;
use core::fmt::{self, Write};
use hal::Console;

struct Stdout;

impl Console for Stdout {
    fn put_byte(&self, byte: u8) {
        console_putchar(byte as usize);
    }
}

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.put_byte(byte);
        }
        Ok(())
    }
}

pub fn print(args: fmt::Arguments) {
    Stdout.write_fmt(args).unwrap();
}

/// Write one byte, without assuming UTF-8: a program may name any byte.
pub fn put_byte(byte: u8) {
    Stdout.put_byte(byte);
}

#[macro_export]
macro_rules! print {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!($fmt $(, $($arg)+)?))
    }
}

#[macro_export]
macro_rules! println {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(format_args!(concat!($fmt, "\n") $(, $($arg)+)?))
    }
}
