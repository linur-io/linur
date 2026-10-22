// SPDX-License-Identifier: GPL-3.0-only
//! The x86_64 smoke kernel: 16550 console, isa-debug-exit, and the marker
//! sequence from `docs/x86_64-smoke.md` section 4.
#![no_std]
#![no_main]

use core::arch::global_asm;
use hal::{Console, Exit};

global_asm!(include_str!("boot.asm"), options(att_syntax));

/// COM1 base port.
const COM1: u16 = 0x3f8;
/// Line status bit: the transmit holding register is empty.
const LSR_THR_EMPTY: u8 = 1 << 5;
/// The QEMU isa-debug-exit port.
const EXIT_PORT: u16 = 0x501;

/// The 16550 UART on COM1.
struct Uart;

impl Uart {
    /// Bring the UART up: 115200 8N1, FIFO on, interrupts off.
    fn new() -> Self {
        outb(COM1 + 1, 0x00); // IER: no interrupts
        outb(COM1 + 3, 0x80); // LCR: divisor latch access
        outb(COM1, 0x01); // divisor low: 115200 baud
        outb(COM1 + 1, 0x00); // divisor high
        outb(COM1 + 3, 0x03); // LCR: 8N1
        outb(COM1 + 2, 0xc7); // FCR: FIFO on, cleared, 14-byte threshold
        outb(COM1 + 4, 0x03); // MCR: DTR and RTS
        Self
    }
}

impl Console for Uart {
    fn put_byte(&self, byte: u8) {
        while inb(COM1 + 5) & LSR_THR_EMPTY == 0 {
            core::hint::spin_loop();
        }
        outb(COM1, byte);
    }
}

/// The QEMU isa-debug-exit device: one 32-bit write to its port stops the
/// machine. QEMU reports host status `(value << 1) | 1`.
struct DebugExit;

impl Exit for DebugExit {
    fn exit(&self, code: i32) -> ! {
        let value: u32 = if code == 0 { 0x10 } else { 0x11 };
        outl(EXIT_PORT, value);
        loop {
            unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack, preserves_flags)) }
        }
    }
}

/// Write one byte to a port.
fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

/// Write one 32-bit word to a port.
fn outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
    }
}

/// Read one byte from a port.
fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// Write one line to the console.
fn print_line(uart: &Uart, line: &str) {
    for &byte in line.as_bytes() {
        uart.put_byte(byte);
    }
    uart.put_byte(b'\n');
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> ! {
    let uart = Uart::new();
    print_line(&uart, "[x86_64] long mode");
    print_line(&uart, "[x86_64] serial ok");
    print_line(&uart, "[x86_64] exit 0");
    DebugExit.exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    let uart = Uart::new();
    print_line(&uart, "[x86_64] panic");
    DebugExit.exit(1)
}
