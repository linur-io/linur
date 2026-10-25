// SPDX-License-Identifier: GPL-3.0-only
//! Open files, descriptor slots and the kernel VFS (docs/vfs.md §4, §5).
mod inode;
mod pipe;
mod stat;
mod stdio;
mod vfs;

use crate::errno::{ENOTDIR, ESPIPE};
use crate::mm::UserBuffer;
use alloc::sync::Arc;

/// An open file, shared by every descriptor that refers to it.
pub trait File: Send + Sync {
    /// Whether `read` may be called.
    fn readable(&self) -> bool;
    /// Whether `write` may be called.
    fn writable(&self) -> bool;
    /// Read into `buf`, returning the number of bytes read.
    fn read(&self, buf: UserBuffer) -> Result<usize, isize>;
    /// Write from `buf`, returning the number of bytes written.
    fn write(&self, buf: UserBuffer) -> Result<usize, isize>;
    /// Reposition the file offset.
    fn lseek(&self, _offset: isize, _whence: u32) -> Result<usize, isize> {
        Err(ESPIPE)
    }
    /// Copy directory entries into `buf`.
    fn getdents(&self, _buf: UserBuffer) -> Result<usize, isize> {
        Err(ENOTDIR)
    }
    /// The file's metadata.
    fn stat(&self) -> Result<Stat, isize>;
}

/// One slot of a process fd table: the open file and its `CLOEXEC` bit.
#[derive(Clone)]
pub struct FileDescriptor {
    /// The open file, shared by every descriptor that refers to it.
    pub file: Arc<dyn File>,
    /// Whether `exec` closes this descriptor.
    pub cloexec: bool,
}

impl FileDescriptor {
    /// Wrap an open file with its `CLOEXEC` bit.
    pub fn new(file: Arc<dyn File>, cloexec: bool) -> Self {
        Self { file, cloexec }
    }
}

pub use inode::{OSInode, list_apps};
pub use pipe::make_pipe;
pub use stat::Stat;
pub use stdio::{Stdin, Stdout};
pub use vfs::{
    Dentry, Inode, OpenFlags, absolute_path, lookup, lookup_parent, open, read_all, root, stat,
};
