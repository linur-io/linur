// SPDX-License-Identifier: GPL-3.0-only
//! The Linux riscv64 `struct stat` (docs/vfs.md §8).

/// The file-type mask of `st_mode`.
pub const S_IFMT: u32 = 0o170000;
/// A regular file.
pub const S_IFREG: u32 = 0o100000;
/// A directory.
// ABI value; the kernel reads a directory from the disk inode instead.
#[allow(dead_code)]
pub const S_IFDIR: u32 = 0o040000;
/// A character device.
pub const S_IFCHR: u32 = 0o020000;
/// A block device.
pub const S_IFBLK: u32 = 0o060000;
/// A FIFO.
pub const S_IFIFO: u32 = 0o010000;

/// File metadata, laid out as Linux riscv64 does, 128 bytes in total.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stat {
    /// Device number; one filesystem in M2.3.
    pub st_dev: u64,
    /// Inode number within the device.
    pub st_ino: u64,
    /// File type and permission bits.
    pub st_mode: u32,
    /// Hard-link count.
    pub st_nlink: u32,
    /// Owner.
    pub st_uid: u32,
    /// Group.
    pub st_gid: u32,
    /// Device number of a special file.
    pub st_rdev: u64,
    /// Padding.
    pub __pad1: u64,
    /// Size in bytes.
    pub st_size: i64,
    /// Preferred block size for I/O.
    pub st_blksize: i32,
    /// Padding.
    pub __pad2: i32,
    /// Blocks allocated, in 512-byte units.
    pub st_blocks: i64,
    /// Access time; 0 until M3.5.
    pub st_atime: i64,
    /// Access time, nanoseconds.
    pub st_atime_nsec: i64,
    /// Modification time; 0 until M3.5.
    pub st_mtime: i64,
    /// Modification time, nanoseconds.
    pub st_mtime_nsec: i64,
    /// Change time; 0 until M3.5.
    pub st_ctime: i64,
    /// Change time, nanoseconds.
    pub st_ctime_nsec: i64,
    /// Reserved.
    pub __unused4: u32,
    /// Reserved.
    pub __unused5: u32,
}

const _: () = assert!(core::mem::size_of::<Stat>() == 128);
