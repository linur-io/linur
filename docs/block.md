# linur — the block layer and the buffer cache (M2.4)

Status: interface contract for M2.4. Read with `spec/02` (M2.4), `spec/01` §3
and §5 (F6), `spec/03` §5/§6, `docs/vfs.md` and `docs/perm.md`. M2.4 replaces the
`virtio-drivers` crate with our own virtio-blk driver, gives the buffer cache the
shape 0.11 has (hash queues, delayed write), stops the DMA memory from being taken
from the page allocator, retires the `spin` crate from our own code, and closes
the directory-block leak M2.2 documented.

## 1. Scope

In: the own virtio-blk driver over the legacy virtio-MMIO interface; its split
virtqueue (descriptor table, available and used rings, a descriptor free list) and
its request control block; DMA memory in the kernel image instead of page frames;
the kernel's own spinlock (`os/src/sync/spinlock.rs`) and the filesystem crate's own
spinlock (`fs/src/sync.rs`); a buffer cache with hash queues, an eviction policy that
skips pinned caches, and delayed write; `block_cache_sync_all()` as the flush the
future `sync` call will use; directory data-block reclamation on removal.

Out: the `sync` syscall (M6.1, `spec/03` row 36; M2.4 delivers the flush it calls),
a periodic flusher, IRQ-driven I/O, request batching, an elevator or read-ahead
(0.11 features outside the documented target), the x86_64 ATA driver (M5.4),
partition tables and multiple devices (v1.0 has neither).

## 2. The device

QEMU `virt` exposes the block drive as a **legacy (version 1)** virtio-MMIO
device at `0x1000_1000`, already in the board's MMIO list. This is measured, not
assumed: probing the slot on this host reports `magic 0x74726976, version 1,
device id 2, queue_num_max 1024, blk_size 512, capacity 32768` sectors
(`notes/m24.md` pitfall 1 has the probe and its output), the retired
`virtio-drivers` crate accepted only `version == 1`, and QEMU's virtio-mmio
presents the legacy register interface unless the machine is told otherwise.

The registers the driver touches (legacy offsets, 32-bit, volatile):

| offset | register | use |
|---|---|---|
| 0x000 | `MagicValue` | must read 0x7472_6976 |
| 0x004 | `Version` | must read 1 (legacy) |
| 0x008 | `DeviceID` | must read 2 (block) |
| 0x010/0x014 | `DeviceFeatures` + `Sel` | not read: no optional feature is wanted |
| 0x020/0x024 | `DriverFeatures` + `Sel` | write 0: nothing is accepted |
| 0x028 | `GuestPageSize` | 4096 (legacy only) |
| 0x030 | `QueueSel` | 0 |
| 0x034 | `QueueNumMax` | read, must be at least `QUEUE_SIZE` |
| 0x038 | `QueueNum` | `QUEUE_SIZE` |
| 0x03c | `QueueAlign` | 4096: the used ring starts on the second page |
| 0x040 | `QueuePFN` | the queue's physical address >> 12 |
| 0x050 | `QueueNotify` | write 0 after publishing an available entry |
| 0x070 | `Status` | device status bits |
| 0x100 | config: `capacity` `u64` | device size in 512-byte sectors |
| 0x114 | config: `blk_size` `u32` | must read 512 |

Init order: status 0 (reset) → `ACKNOWLEDGE` → `ACKNOWLEDGE|DRIVER` → driver
features 0 → `GuestPageSize` 4096 → queue 0 configured
(`QueueNum`, `QueueAlign`, `QueuePFN`) → `...|FEATURES_OK|DRIVER_OK`. The driver
probes the slot at construction and panics when the magic, version or device id do
not match: there is no fallback filesystem, so a missing disk is a boot failure,
not an error path.

## 3. Requests and the request queue

`os/src/drivers/block/virtio_blk.rs` implements `easy_fs::BlockDevice` unchanged
(`read_block`/`write_block`, a panic on a failed request: the trait has no error
channel and a broken disk cannot be papered over).

One request is a chain of three descriptors:

1. the header, read-only: `{ type: u32, reserved: u32, sector: u64 }`
   (`type` 0 reads the disk into the buffer, 1 writes the buffer to the disk;
   `sector = block_id`, because the block size is 512);
2. the data buffer, 512 bytes: device-writable (`VIRTQ_DESC_F_WRITE`) for a read,
   device-readable for a write;
3. the status byte, device-writable: 0 is success.

The **request queue** is the split virtqueue: a `QUEUE_SIZE` = 8 descriptor table
with a free list threaded through `next`, the available ring the driver fills, the
used ring the device fills, and the index state (`free_head`, `avail_idx`,
`last_used`). Publishing a chain is: pop three descriptors from the free list, fill
them, put the head in `avail.ring[avail_idx & 7]`, fence, `avail_idx += 1`, write
`QueueNotify`. Completing is: poll `used.idx` (volatile) until it advances, fence,
read the used element whose `id` is the chain head, check the status byte, push
the three descriptors back onto the free list. The rings can hold two chains at
once; `read_block`/`write_block` submit one chain and wait for it, which is what
the synchronous buffer cache needs (batching a flush is post-v1.0).

The data buffer must be 512-byte aligned so that one descriptor cannot cross a
page: the buffer cache allocates its blocks with a 512-byte alignment for exactly
this reason, and the driver asserts it.

## 4. DMA memory

The descriptor table, both rings and the request control block live in **one
page-aligned static area in the kernel image** (`DmaArea`, 8 KiB: the free list and
available ring in the first page, the used ring at the 4096-byte boundary, the
control block after it). Physical addresses come from the kernel page table
(identity today). Nothing is allocated from the page-frame allocator, so the
previous driver's `QUEUE_FRAMES` global and the frames it held disappear: the DMA
footprint is a fixed part of the image and the "DMA frames leaked" defect is
closed by construction rather than by a better free path. The area is a singleton,
like the one device.

The data buffers themselves are the cache's heap buffers (kernel-mapped, one page
each by the alignment rule above).

## 5. The block layer

`easy_fs::BlockDevice` keeps its shape. `os/src/drivers/block/mod.rs` keeps the
`BLOCK_DEVICE` singleton built from the board's `BlockDeviceImpl`, and the MMIO
page stays in the board's `MMIO` list, so the arch-specific address does not leak
into the driver.

## 6. Locks

Two small spinlocks replace `spin` in our own code:

```rust
// os/src/sync/spinlock.rs and fs/src/sync.rs, same shape
pub struct SpinMutex<T> { locked: AtomicBool, data: UnsafeCell<T> }
impl<T> SpinMutex<T> {
    pub const fn new(data: T) -> Self;
    pub fn lock(&self) -> SpinMutexGuard<'_, T>;   // Acquire; spin_loop while held
}
// the guard releases with Release and derefs to T
```

* The kernel one guards the driver's queue state. It is taken in syscall context,
  where the driver polls a completion; no interrupt handler touches it, so it does
  not need to disable interrupts (M3.1's device layer revisits that).
* easy-fs cannot depend on the kernel, so it carries the same lock for the cache
  manager, each cache and `EasyFileSystem`; the two implementations are
  deliberately separate and documented as such.
* `spin` leaves the manifests we control: `fs/Cargo.toml` drops it, and
  `os` never listed it. The kernel graph keeps `spin 0.7.2` only inside
  `buddy_system_allocator` (retired at M2.6) and `spin 0.9.9` inside
  `lazy_static`; `DEPENDENCIES.md` and `deny.toml` are updated to say exactly that.
  `virtio-drivers`, `volatile` and `log` leave the graph with the driver.

## 7. The buffer cache

`fs/src/block_cache.rs`, rewritten:

* **Hash queues.** `BLOCK_CACHE_SIZE` = 64 caches in `HASH_BUCKETS` = 16 buckets,
  `bucket = block_id % HASH_BUCKETS`. A hit is a search of that bucket only.
  A miss takes the oldest unpinned cache (strong count 1) of the bucket, or the
  oldest unpinned cache of the next buckets in order; the front of each
  `VecDeque` is the oldest, which is the free list 0.11 keeps. A miss with every
  cache pinned is a kernel bug and panics; one-block-at-a-time inode work never
  holds more than one inode block at a time.
* **Delayed write.** A modified cache is dirty; it reaches the device when it is
  evicted, when its own `sync()` runs, or when `block_cache_sync_all()` runs.
  The filesystem mutators no longer flush, so a rename no longer costs a disk
  write per dirty block. A later read of a dirty block is served from the cache,
  which keeps the in-RAM filesystem coherent; a crash loses what has not been
  evicted or flushed, and v1.0 makes no crash-consistency claim.
* **Flush points.** `block_cache_sync_all()` is public. The host packer calls it
  before exiting (an image that is not flushed is not an image), the host test
  calls it where it wants a device write, and the `sync` syscall of M6.1 will call
  it — the "`sync`" of the M2.4 plan row is this flush, while the syscall number
  itself stays M6.1 (`spec/03` row 36). There is no periodic flusher before
  M3.5/M6.1.

## 8. Directory block reclamation

`docs/vfs.md` §3 records the M2.2 limitation: removing directory entries shrinks
the inode's size but keeps the data blocks, so a directory that grows past a block
boundary and shrinks again leaks a block. M2.4 fixes it:

* `DiskInode::shrink_size(new_size, block_device) -> Vec<u32>` frees every block
  the new size no longer covers — data blocks from the direct array, then the
  indirect1 entries, then the indirect2 entries and their indirect1 tables — and
  clears the pointers. It is the mirror of `increase_size`/`clear_size`.
* `Inode::remove_entry` (used by `remove` and `rename`) calls it and returns the
  blocks to the data bitmap, so the block count follows the directory size in both
  directions.
* `EasyFileSystem::free_data_blocks()` counts the unallocated data blocks (the
  zero bits of the data bitmap). The host test asserts reclamation with it, and it
  is the accounting a future `statfs` wants.

## 9. Host tests

`fs/host/tests/efs.rs`'s single `efs_test` (moved out of `easy-fs-fuse` at M2.7; see
`notes/m23.md` pitfall 4: the block cache is process-global, so one test owns it)
grows these cases. The host adapter's `BlockFile` counts device reads and writes.

* **Delayed write:** create and size the file *before* the baseline, then
  overwrite the same resident blocks; the device read and write counts do not
  move until `block_cache_sync_all()`, which moves the write count by at least the
  four dirtied blocks. Creating a file after the baseline would dirty its parent
  and can evict an unrelated dirty block from a full cache (`notes/m24.md`
  pitfall 7).
* **Hash hits:** read a file, record the device read count, read it again; the
  second pass issues no device reads.
* **Capacity:** write more than `BLOCK_CACHE_SIZE` blocks across files, flush,
  read everything back and compare (eviction, write-back and reload), including a
  file that spans the direct/indirect boundaries.
* **Reclamation:** fill a directory past one 512-byte block of entries (52
  entries, four data blocks), remove entries until it shrinks to one block, and
  assert `free_data_blocks()` grew by the three freed blocks; then a second case
  with 2502 entries (160 blocks, i.e. indirect2 plus its indirect1 table) shrunk
  back under the 154-block indirect1 boundary, where the removal must free three
  data blocks, the table and both indirect blocks (five in total).
* **Metadata:** the M2.3 cases stay (mode/uid/gid/dev round-trip).

## 10. Kernel tests

One new program, `user/src/bin/disk_stress_test.rs`: build a directory tree,
write ~100 files with checkable content scattered over more than
`BLOCK_CACHE_SIZE` distinct blocks, read everything back and compare, unlink half,
re-read the rest, then remove the tree. It exercises the driver, the request
queue, eviction and delayed write from the user side. It joins `SUCC_TESTS`, and
the gate count grows 75 → 76 (`scripts/verify.sh`).

## 11. Deliberately not in M2.4

The `sync` syscall (M6.1), a periodic flusher, crash consistency, IRQ-driven
completion, request batching/elevator/read-ahead, multiple devices or partition
parsing, the x86_64 ATA driver (M5.4), and shared/uncached DMA mappings (the
kernel is identity mapped and QEMU makes the device coherent with it).

## 12. Errors

No new errno. A failed device request panics with the status byte; the filesystem
keeps the errno mapping of `docs/perm.md` §10. A cache miss with every cache
pinned panics with the cache count.
