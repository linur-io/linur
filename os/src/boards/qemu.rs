// SPDX-License-Identifier: GPL-3.0-only
use hal::Exit;

pub const CLOCK_FREQ: usize = 12500000;
/// Start of the board's RAM (the frame map is sized from here, docs/alloc.md §4).
pub const MEMORY_START: usize = 0x8000_0000;
pub const MEMORY_END: usize = 0x8800_0000;

pub const MMIO: &[(usize, usize)] = &[
    (0x0010_0000, 0x00_2000), // VIRT_TEST/RTC  in virt machine
    (0x1000_1000, 0x00_1000), // Virtio Block in virt machine
];

/// The virtio-MMIO slot that carries the block drive (docs/block.md §2).
pub const VIRTIO0: usize = 0x1000_1000;

pub type BlockDeviceImpl = crate::drivers::block::VirtIOBlock;

/// QEMU virt's SiFive test finisher, already mapped through `MMIO`.
const TEST_DEVICE: usize = 0x0010_0000;

/// QEMU virt's board: the SiFive test finisher is its stop primitive.
struct Board;

impl Exit for Board {
    fn exit(&self, code: i32) -> ! {
        let status = if code == 0 {
            0x5555u32
        } else {
            ((code as u32) << 16) | 0x3333
        };
        unsafe { (TEST_DEVICE as *mut u32).write_volatile(status) };
        loop {
            unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
        }
    }
}

/// Stop QEMU with `code`: 0 signals a pass, anything else a failure.
pub fn exit(code: i32) -> ! {
    Board.exit(code)
}
