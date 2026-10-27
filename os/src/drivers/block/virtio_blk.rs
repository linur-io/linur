// SPDX-License-Identifier: GPL-3.0-only
//! The own virtio-blk driver over the legacy virtio-MMIO interface
//! (docs/block.md §2-§4).

use super::BlockDevice;
use crate::board::VIRTIO0;
use crate::config::PAGE_SIZE;
use crate::mm::{PageTable, VirtAddr, kernel_token};
use crate::sync::SpinMutex;
use core::hint::spin_loop;
use core::mem::{offset_of, size_of};
use core::sync::atomic::{Ordering, fence};

/// Descriptors and ring entries the queue holds; a power of two.
const QUEUE_SIZE: usize = 8;
/// The block size the on-disk format is built on.
const BLOCK_SIZE: usize = 512;
/// Legacy `QueueAlign`: the used ring starts on this boundary.
const QUEUE_ALIGN: usize = 4096;

/// The magic value every virtio-MMIO device reports.
const MAGIC_VALUE: u32 = 0x7472_6976;
/// The device id of a block device.
const DEVICE_ID_BLOCK: u32 = 2;

/// Acknowledge the device.
const STATUS_ACKNOWLEDGE: u32 = 1;
/// The driver knows how to drive it.
const STATUS_DRIVER: u32 = 2;
/// The driver accepted the feature set.
const STATUS_FEATURES_OK: u32 = 8;
/// The driver is ready.
const STATUS_DRIVER_OK: u32 = 4;

/// The descriptor continues the chain.
const DESC_NEXT: u16 = 1;
/// The device writes the buffer instead of reading it.
const DESC_WRITE: u16 = 2;

/// Request type: read the disk into the data buffer.
const REQ_IN: u32 = 0;
/// Request type: write the data buffer to the disk.
const REQ_OUT: u32 = 1;

// Legacy virtio-MMIO register offsets (docs/block.md §2).
const REG_MAGIC: usize = 0x000;
const REG_VERSION: usize = 0x004;
const REG_DEVICE_ID: usize = 0x008;
const REG_DRIVER_FEATURES: usize = 0x020;
const REG_GUEST_PAGE_SIZE: usize = 0x028;
const REG_QUEUE_SEL: usize = 0x030;
const REG_QUEUE_NUM_MAX: usize = 0x034;
const REG_QUEUE_NUM: usize = 0x038;
const REG_QUEUE_ALIGN: usize = 0x03c;
const REG_QUEUE_PFN: usize = 0x040;
const REG_QUEUE_NOTIFY: usize = 0x050;
const REG_STATUS: usize = 0x070;
/// Config space: the device size in 512-byte sectors.
const CONFIG_CAPACITY: usize = 0x100;
/// Config space: the device's logical block size.
const CONFIG_BLK_SIZE: usize = 0x114;

/// Read one 32-bit MMIO register.
fn reg_read(offset: usize) -> u32 {
    unsafe { ((VIRTIO0 + offset) as *const u32).read_volatile() }
}

/// Write one 32-bit MMIO register.
fn reg_write(offset: usize, value: u32) {
    unsafe { ((VIRTIO0 + offset) as *mut u32).write_volatile(value) }
}

/// Read one 32-bit device configuration word.
fn config_read_u32(offset: usize) -> u32 {
    unsafe { ((VIRTIO0 + offset) as *const u32).read_volatile() }
}

/// Read one 64-bit device configuration word.
fn config_read_u64(offset: usize) -> u64 {
    unsafe { ((VIRTIO0 + offset) as *const u64).read_volatile() }
}

/// The physical address behind a kernel virtual address.
fn phys_addr(va: usize) -> usize {
    PageTable::from_token(kernel_token())
        .translate_va(VirtAddr::from(va))
        .unwrap()
        .0
}

/// One descriptor of the split virtqueue.
#[repr(C)]
#[derive(Clone, Copy)]
struct Descriptor {
    /// Bus address of the buffer.
    addr: u64,
    /// Buffer length in bytes.
    len: u32,
    /// `DESC_NEXT` and/or `DESC_WRITE`.
    flags: u16,
    /// The next descriptor of the chain.
    next: u16,
}

impl Descriptor {
    /// The descriptor of a table that has not been filled in.
    const ZERO: Self = Self {
        addr: 0,
        len: 0,
        flags: 0,
        next: 0,
    };
}

/// The available ring: the driver publishes chain heads here for the device.
#[repr(C)]
struct AvailRing {
    /// Interrupt suppression flags; never used.
    flags: u16,
    /// Incremented for every published chain.
    idx: u16,
    /// The chain heads, indexed by `idx & (QUEUE_SIZE - 1)`.
    ring: [u16; QUEUE_SIZE],
}

impl AvailRing {
    /// An available ring that has published nothing.
    const ZERO: Self = Self {
        flags: 0,
        idx: 0,
        ring: [0; QUEUE_SIZE],
    };
}

/// One completion of the used ring.
#[repr(C)]
#[derive(Clone, Copy)]
struct UsedElement {
    /// The chain head that completed.
    id: u32,
    /// Bytes the device wrote into the chain's writable buffers.
    len: u32,
}

impl UsedElement {
    /// The completion of a ring that has completed nothing.
    const ZERO: Self = Self { id: 0, len: 0 };
}

/// The used ring: the device publishes completions here.
#[repr(C)]
struct UsedRing {
    /// Interrupt suppression flags; never used.
    flags: u16,
    /// Incremented for every completed chain.
    idx: u16,
    /// The completions, indexed by `idx & (QUEUE_SIZE - 1)`.
    ring: [UsedElement; QUEUE_SIZE],
}

impl UsedRing {
    /// A used ring that has completed nothing.
    const ZERO: Self = Self {
        flags: 0,
        idx: 0,
        ring: [UsedElement::ZERO; QUEUE_SIZE],
    };
}

/// The header the device reads before a request's data.
#[repr(C)]
#[derive(Clone, Copy)]
struct RequestHeader {
    /// `REQ_IN` or `REQ_OUT`.
    type_: u32,
    /// Padding.
    reserved: u32,
    /// The first sector of the request.
    sector: u64,
}

impl RequestHeader {
    /// The header of a request that has not been filled in.
    const ZERO: Self = Self {
        type_: 0,
        reserved: 0,
        sector: 0,
    };
}

/// Bytes between the available ring and the `QueueAlign` boundary.
const RING_PAD: usize = QUEUE_ALIGN - size_of::<Descriptor>() * QUEUE_SIZE - size_of::<AvailRing>();

/// The memory the device DMAs into (docs/block.md §4): the descriptor table and
/// the available ring in the first page, the used ring on the next page boundary
/// (the legacy `QueueAlign`), the request control block after it.
#[repr(C, align(4096))]
struct DmaArea {
    desc: [Descriptor; QUEUE_SIZE],
    avail: AvailRing,
    /// Padding up to the boundary the used ring must start on.
    _pad: [u8; RING_PAD],
    used: UsedRing,
    header: RequestHeader,
    status: u8,
}

// The device expects the used ring exactly at the QueueAlign boundary.
const _: () = assert!(offset_of!(DmaArea, used) == QUEUE_ALIGN);
const _: () = assert!(size_of::<DmaArea>() <= 2 * QUEUE_ALIGN);

/// The one DMA area: a fixed part of the kernel image, so there is nothing to
/// allocate and nothing to leak (docs/block.md §4).
static mut DMA: DmaArea = DmaArea {
    desc: [Descriptor::ZERO; QUEUE_SIZE],
    avail: AvailRing::ZERO,
    _pad: [0; RING_PAD],
    used: UsedRing::ZERO,
    header: RequestHeader::ZERO,
    status: 0,
};

/// The DMA area, borrowed for one operation while the queue lock is held.
fn dma_area() -> &'static mut DmaArea {
    unsafe { &mut *core::ptr::addr_of_mut!(DMA) }
}

/// The request queue: the descriptor free list and both ring cursors
/// (docs/block.md §3).
struct RequestQueue {
    /// Head of the descriptor free list.
    free_head: u16,
    /// Next available-ring slot to publish.
    avail_idx: u16,
    /// Next used-ring slot to reap.
    last_used: u16,
}

impl RequestQueue {
    /// A queue that has published nothing.
    const fn new() -> Self {
        Self {
            free_head: 0,
            avail_idx: 0,
            last_used: 0,
        }
    }

    /// Take the next three descriptors off the free list.
    fn allocate(&mut self, dma: &mut DmaArea) -> [u16; 3] {
        let first = self.free_head;
        let second = dma.desc[first as usize].next;
        let third = dma.desc[second as usize].next;
        self.free_head = dma.desc[third as usize].next;
        [first, second, third]
    }

    /// Put a completed chain back at the front of the free list.
    fn recycle(&mut self, dma: &mut DmaArea, chain: [u16; 3]) {
        dma.desc[chain[2] as usize].next = self.free_head;
        self.free_head = chain[0];
    }

    /// Publish `chain` in the available ring and notify the device.
    fn publish(&mut self, dma: &mut DmaArea, chain: [u16; 3]) {
        let slot = (self.avail_idx & (QUEUE_SIZE as u16 - 1)) as usize;
        dma.avail.ring[slot] = chain[0];
        self.avail_idx = self.avail_idx.wrapping_add(1);
        // the device must see the chain and the new idx before the notification
        unsafe { core::ptr::write_volatile(&mut dma.avail.idx, self.avail_idx) };
        fence(Ordering::SeqCst);
        reg_write(REG_QUEUE_NOTIFY, 0);
    }

    /// Wait for `chain` to complete and return its status byte.
    fn wait(&mut self, dma: &mut DmaArea, chain: [u16; 3]) -> u8 {
        while unsafe { core::ptr::read_volatile(&dma.used.idx) } == self.last_used {
            spin_loop();
        }
        fence(Ordering::SeqCst);
        let slot = (self.last_used & (QUEUE_SIZE as u16 - 1)) as usize;
        let element = unsafe { core::ptr::read_volatile(&dma.used.ring[slot]) };
        assert_eq!(
            element.id as u16, chain[0],
            "virtio-blk completed an unexpected request"
        );
        self.last_used = self.last_used.wrapping_add(1);
        unsafe { core::ptr::read_volatile(&dma.status) }
    }
}

/// The mutable state of the one device.
struct Device {
    queue: RequestQueue,
}

impl Device {
    /// Probe the MMIO slot and initialise the device and its queue 0
    /// (docs/block.md §2).
    fn new() -> Self {
        if reg_read(REG_MAGIC) != MAGIC_VALUE
            || reg_read(REG_VERSION) != 1
            || reg_read(REG_DEVICE_ID) != DEVICE_ID_BLOCK
        {
            panic!("no legacy virtio-blk device at {VIRTIO0:#x}");
        }
        // reset, then the legacy handshake; no optional feature is accepted
        reg_write(REG_STATUS, 0);
        reg_write(REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);
        reg_write(REG_DRIVER_FEATURES, 0);
        reg_write(REG_GUEST_PAGE_SIZE, PAGE_SIZE as u32);
        reg_write(REG_QUEUE_SEL, 0);
        let max = reg_read(REG_QUEUE_NUM_MAX);
        assert!(
            max as usize >= QUEUE_SIZE,
            "queue 0 holds {max} entries, {QUEUE_SIZE} needed"
        );
        // every descriptor starts on the free list
        let dma = dma_area();
        for (index, desc) in dma.desc.iter_mut().enumerate() {
            desc.next = ((index + 1) % QUEUE_SIZE) as u16;
        }
        // the queue lives in the DMA area, so the device only needs its page frame
        let queue_pfn = phys_addr(core::ptr::addr_of!(DMA) as usize) / PAGE_SIZE;
        reg_write(REG_QUEUE_NUM, QUEUE_SIZE as u32);
        reg_write(REG_QUEUE_ALIGN, QUEUE_ALIGN as u32);
        reg_write(REG_QUEUE_PFN, queue_pfn as u32);
        reg_write(
            REG_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );
        let blk_size = config_read_u32(CONFIG_BLK_SIZE);
        assert_eq!(
            blk_size as usize, BLOCK_SIZE,
            "the on-disk format needs 512-byte blocks, the device has {blk_size}"
        );
        let capacity = config_read_u64(CONFIG_CAPACITY);
        println!(
            "[block] virtio-blk: {} sectors, {} MiB",
            capacity,
            capacity / 2048
        );
        Self {
            queue: RequestQueue::new(),
        }
    }
}

/// The own virtio-blk driver (docs/block.md §2).
pub struct VirtIOBlock {
    inner: SpinMutex<Device>,
}

impl VirtIOBlock {
    /// Bring the device up.
    pub fn new() -> Self {
        Self {
            inner: SpinMutex::new(Device::new()),
        }
    }

    /// Submit one request for `len` bytes at `addr` and wait for it
    /// (docs/block.md §3).
    fn transfer(&self, block_id: usize, addr: usize, len: usize, write: bool) {
        assert_eq!(len, BLOCK_SIZE, "a request moves one block");
        // one descriptor must not cross a page, so the buffer is block aligned
        assert_eq!(
            addr % BLOCK_SIZE,
            0,
            "a virtio-blk buffer must be 512-byte aligned"
        );
        let mut device = self.inner.lock();
        let dma = dma_area();
        let chain = device.queue.allocate(dma);
        dma.header = RequestHeader {
            type_: if write { REQ_OUT } else { REQ_IN },
            reserved: 0,
            sector: block_id as u64,
        };
        // the status trail must not look like a success before the device writes it
        dma.status = 0xff;
        let header_pa = phys_addr(core::ptr::addr_of!(dma.header) as usize) as u64;
        let status_pa = phys_addr(core::ptr::addr_of!(dma.status) as usize) as u64;
        dma.desc[chain[0] as usize] = Descriptor {
            addr: header_pa,
            len: size_of::<RequestHeader>() as u32,
            flags: DESC_NEXT,
            next: chain[1],
        };
        dma.desc[chain[1] as usize] = Descriptor {
            addr: phys_addr(addr) as u64,
            len: len as u32,
            flags: DESC_NEXT | if write { 0 } else { DESC_WRITE },
            next: chain[2],
        };
        dma.desc[chain[2] as usize] = Descriptor {
            addr: status_pa,
            len: 1,
            flags: DESC_WRITE,
            next: 0,
        };
        device.queue.publish(dma, chain);
        let status = device.queue.wait(dma, chain);
        assert_eq!(
            status, 0,
            "virtio-blk request for block {block_id} failed with status {status}"
        );
        device.queue.recycle(dma, chain);
    }
}

impl BlockDevice for VirtIOBlock {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        self.transfer(block_id, buf.as_mut_ptr() as usize, buf.len(), false);
    }

    fn write_block(&self, block_id: usize, buf: &[u8]) {
        self.transfer(block_id, buf.as_ptr() as usize, buf.len(), true);
    }
}
