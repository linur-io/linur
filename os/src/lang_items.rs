// SPDX-License-Identifier: GPL-3.0-only
use crate::sbi::shutdown;
use core::arch::asm;
use core::panic::PanicInfo;

unsafe extern "C" {
    safe fn stext();
    safe fn etext();
}

/// Whether an address falls inside the kernel text, where the walk may continue.
fn in_kernel_text(addr: usize) -> bool {
    let (start, end) = (linker_symbol_addr!(stext), linker_symbol_addr!(etext));
    (start..end).contains(&addr)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    match info.location() {
        Some(location) => error!(
            "[kernel] panic at {}:{}: {}",
            location.file(),
            location.line(),
            info.message()
        ),
        None => error!("[kernel] panic: {}", info.message()),
    }
    backtrace();
    shutdown(true)
}

/// Walk the frame-pointer chain and print each return address, outermost call last.
///
/// The kernel is built with `-Cforce-frame-pointers=yes`: a frame records the
/// return address at `fp - 8` and the caller's `fp` at `fp - 16`. The chain ends
/// at 0 (the outermost task context) or when a frame pointer stops advancing.
/// The addresses resolve with `make symbolize`.
fn backtrace() {
    let mut fp: usize;
    unsafe {
        asm!("mv {}, s0", out(reg) fp);
    }
    println!("---START BACKTRACE---");
    for depth in 0..16 {
        if fp == 0 || fp % 16 != 0 {
            break;
        }
        let (ra, next) = unsafe { (*((fp - 8) as *const usize), *((fp - 16) as *const usize)) };
        if !in_kernel_text(ra) {
            // The kernel frame chain ends where the trap entry assembly begins; beyond it
            // the saved values belong to the interrupted user context.
            break;
        }
        println!("#{}: ra={:#x}", depth, ra);
        if next <= fp {
            break;
        }
        fp = next;
    }
    println!("---END   BACKTRACE---");
}
