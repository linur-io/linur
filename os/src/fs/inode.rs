// SPDX-License-Identifier: GPL-3.0-only
//! Open-file objects over VFS inodes (docs/vfs.md §5, §9).

use super::vfs;
use super::{File, Inode, Stat};
use crate::errno::{EINVAL, EISDIR, ENOTDIR};
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// `SEEK_SET`: the offset is absolute.
const SEEK_SET: u32 = 0;
/// `SEEK_CUR`: the offset is relative to the current one.
const SEEK_CUR: u32 = 1;
/// `SEEK_END`: the offset is relative to the file size.
const SEEK_END: u32 = 2;
/// `DT_DIR`, a directory, in a `linux_dirent64` record.
const DT_DIR: u8 = 4;
/// `DT_REG`, a regular file, in a `linux_dirent64` record.
const DT_REG: u8 = 8;
/// A `linux_dirent64` header: `d_ino`, `d_off`, `d_reclen` and `d_type`.
const DIRENT64_HEADER: usize = 19;
/// The longest `linux_dirent64` record, rounded up to 8.
const DIRENT64_MAX: usize = 48;

/// One open file over a VFS inode: access bits, append bit and per-open offset.
pub struct OSInode {
    readable: bool,
    writable: bool,
    append: bool,
    inode: Arc<Inode>,
    offset: UPSafeCell<usize>,
}

impl OSInode {
    /// Open `inode` with the per-descriptor state of one open file.
    pub fn new(readable: bool, writable: bool, append: bool, inode: Arc<Inode>) -> Self {
        Self {
            readable,
            writable,
            append,
            inode,
            offset: unsafe { UPSafeCell::new(0) },
        }
    }
}

impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }

    fn writable(&self) -> bool {
        self.writable
    }

    fn read(&self, mut buf: UserBuffer) -> Result<usize, isize> {
        if !self.inode.is_file() {
            return Err(EISDIR);
        }
        let mut offset = self.offset.exclusive_access();
        let mut total = 0;
        for slice in buf.buffers.iter_mut() {
            let len = self.inode.read_at(*offset, slice);
            if len == 0 {
                break;
            }
            *offset += len;
            total += len;
        }
        Ok(total)
    }

    fn write(&self, buf: UserBuffer) -> Result<usize, isize> {
        if !self.inode.is_file() {
            return Err(EISDIR);
        }
        let mut offset = self.offset.exclusive_access();
        if self.append {
            *offset = self.inode.size() as usize;
        }
        let mut total = 0;
        for slice in buf.buffers.iter() {
            let len = self.inode.write_at(*offset, slice);
            *offset += len;
            total += len;
        }
        Ok(total)
    }

    fn lseek(&self, offset: isize, whence: u32) -> Result<usize, isize> {
        let mut file_offset = self.offset.exclusive_access();
        let base = match whence {
            SEEK_SET => 0,
            SEEK_CUR => *file_offset as isize,
            SEEK_END => self.inode.size() as isize,
            _ => return Err(EINVAL),
        };
        let Some(new_offset) = base.checked_add(offset).filter(|offset| *offset >= 0) else {
            return Err(EINVAL);
        };
        *file_offset = new_offset as usize;
        Ok(new_offset as usize)
    }

    fn getdents(&self, buf: UserBuffer) -> Result<usize, isize> {
        if !self.inode.is_dir() {
            return Err(ENOTDIR);
        }
        let capacity = buf.len();
        let mut record = [0u8; DIRENT64_MAX];
        let mut payload = Vec::new();
        let mut offset = self.offset.exclusive_access();
        while let Some(dirent) = self.inode.dirent_at(*offset) {
            let reclen = (DIRENT64_HEADER + dirent.name.len() + 1).div_ceil(8) * 8;
            if payload.len() + reclen > capacity {
                break;
            }
            record.fill(0);
            record[..8].copy_from_slice(&(dirent.inode_id as u64).to_ne_bytes());
            record[8..16].copy_from_slice(&(dirent.next_offset as i64).to_ne_bytes());
            record[16..18].copy_from_slice(&(reclen as u16).to_ne_bytes());
            record[18] = if dirent.is_dir { DT_DIR } else { DT_REG };
            record[DIRENT64_HEADER..DIRENT64_HEADER + dirent.name.len()]
                .copy_from_slice(dirent.name.as_bytes());
            payload.extend_from_slice(&record[..reclen]);
            *offset = dirent.next_offset;
        }
        let mut copied = 0;
        for slice in buf.buffers {
            let len = slice.len().min(payload.len() - copied);
            slice[..len].copy_from_slice(&payload[copied..copied + len]);
            copied += len;
            if copied == payload.len() {
                break;
            }
        }
        Ok(payload.len())
    }

    fn stat(&self) -> Result<Stat, isize> {
        Ok(vfs::stat(&self.inode))
    }
}

/// Print the root directory entries at boot.
pub fn list_apps() {
    println!("/**** APPS ****");
    let root = vfs::root();
    let mut offset = 0;
    while let Some(dirent) = root.inode().dirent_at(offset) {
        if dirent.name != "." && dirent.name != ".." {
            println!("{}", dirent.name);
        }
        offset = dirent.next_offset;
    }
    println!("**************/");
}
