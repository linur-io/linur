// SPDX-License-Identifier: GPL-3.0-only
//! Kernel VFS objects, the path walker, open and the metadata (docs/vfs.md §4,
//! §7; docs/perm.md §5, §6).

use super::OSInode;
use super::mount;
use super::perm::{Cred, MAY_EXEC, MAY_READ, MAY_WRITE, permission};
use super::stat::{S_IFMT, S_IFREG, Stat};
use crate::errno::{EEXIST, EINVAL, EISDIR, ENAMETOOLONG, ENOENT, ENOTDIR, ENOTEMPTY, ENXIO};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use easy_fs::VfsError;

/// Longest single path component, one easy-fs directory entry (docs/vfs.md §2).
const NAME_MAX: usize = 27;
/// Longest whole path.
const PATH_MAX: usize = 4096;
/// The access-mode bits of the open flags.
const O_ACCMODE: u32 = 0o3;

bitflags! {
    /// The Linux `open(2)` flags (docs/vfs.md §7).
    pub struct OpenFlags: u32 {
        /// Open for reading only.
        const RDONLY = 0;
        /// Open for writing only.
        const WRONLY = 0x1;
        /// Open for reading and writing.
        const RDWR = 0x2;
        /// Create a regular file when the path does not exist.
        const CREATE = 0x40;
        /// With `CREATE`, fail when the path already exists.
        const EXCL = 0x80;
        /// Empty an existing regular file.
        const TRUNC = 0x200;
        /// Write at the end of the file whatever the offset is.
        const APPEND = 0x400;
        /// Fail unless the path names a directory.
        const DIRECTORY = 0x10000;
        /// Close the descriptor on a successful `exec`.
        const CLOEXEC = 0x80000;
    }
}

impl OpenFlags {
    /// The (readable, writable) pair implied by the access mode.
    pub fn read_write(&self) -> (bool, bool) {
        match self.bits() & O_ACCMODE {
            0 => (true, false),
            1 => (false, true),
            _ => (true, true),
        }
    }
}

/// One name-to-inode binding, as produced by the path walker.
pub struct Dentry {
    name: String,
    inode: Arc<Inode>,
}

impl Dentry {
    /// Bind `name` to `inode`.
    pub(super) fn new(name: String, inode: Arc<Inode>) -> Self {
        Self { name, inode }
    }

    /// The final path component that named this binding.
    // Contract API (docs/vfs.md §4); no caller needs it yet.
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The inode the name resolves to.
    pub fn inode(&self) -> &Arc<Inode> {
        &self.inode
    }
}

/// Kernel inode: the disk inode plus the operations the syscalls need.
pub struct Inode {
    inner: Arc<easy_fs::Inode>,
}

impl Inode {
    /// Wrap a disk-layer inode.
    pub(super) fn new(inner: Arc<easy_fs::Inode>) -> Arc<Self> {
        Arc::new(Self { inner })
    }

    /// The inode number within the filesystem.
    pub fn inode_id(&self) -> u32 {
        self.inner.inode_id()
    }

    /// Whether this inode is a directory.
    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    /// Whether this inode is a regular file.
    pub fn is_file(&self) -> bool {
        self.inner.is_file()
    }

    /// The file type and permission bits.
    pub fn mode(&self) -> u32 {
        self.inner.mode() as u32
    }

    /// The owner.
    pub fn uid(&self) -> u32 {
        self.inner.uid() as u32
    }

    /// The group.
    pub fn gid(&self) -> u32 {
        self.inner.gid() as u32
    }

    /// The device number of a special file; 0 for every other type.
    pub fn dev(&self) -> u32 {
        self.inner.dev()
    }

    /// Replace the file type and permission bits.
    pub fn set_mode(&self, mode: u32) {
        self.inner.set_mode(mode as u16);
    }

    /// Replace the owner and the group.
    pub fn set_owner(&self, uid: u32, gid: u32) {
        self.inner.set_owner(uid as u16, gid as u16);
    }

    /// The file size in bytes.
    pub fn size(&self) -> u32 {
        self.inner.size()
    }

    /// The hard-link count.
    pub fn nlink(&self) -> u32 {
        self.inner.nlink()
    }

    /// Read at `offset`.
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        self.inner.read_at(offset, buf)
    }

    /// Write at `offset`, growing the file.
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        self.inner.write_at(offset, buf)
    }

    /// Free the data blocks and set the size to 0.
    pub fn truncate(&self) {
        self.inner.truncate();
    }

    /// Look up one name in this directory.
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        self.inner.find(name).map(Inode::new)
    }

    /// Read the directory entry at byte `offset`.
    pub fn dirent_at(&self, offset: usize) -> Option<easy_fs::Dirent> {
        self.inner.dirent_at(offset)
    }

    /// Create a regular file in this directory; `perms` are permission bits.
    pub fn create_file(&self, name: &str, perms: u32, cred: Cred) -> Result<Arc<Inode>, isize> {
        self.inner
            .create_file(name, perms as u16, cred.uid as u16, cred.gid as u16)
            .map(Inode::new)
            .map_err(to_errno)
    }

    /// Create a directory in this directory; `perms` are permission bits.
    pub fn create_dir(&self, name: &str, perms: u32, cred: Cred) -> Result<Arc<Inode>, isize> {
        self.inner
            .create_dir(name, perms as u16, cred.uid as u16, cred.gid as u16)
            .map(Inode::new)
            .map_err(to_errno)
    }

    /// Create a special file; `mode` carries the type and the permission bits.
    pub fn create_node(
        &self,
        name: &str,
        mode: u32,
        cred: Cred,
        dev: u32,
    ) -> Result<Arc<Inode>, isize> {
        self.inner
            .create_node(name, mode as u16, cred.uid as u16, cred.gid as u16, dev)
            .map(Inode::new)
            .map_err(to_errno)
    }

    /// Remove the entry `name` from this directory.
    pub fn remove(&self, name: &str) -> Result<(), isize> {
        self.inner.remove(name).map_err(to_errno)
    }

    /// Link `target` into this directory as `name`.
    pub fn link(&self, name: &str, target: &Arc<Inode>) -> Result<(), isize> {
        self.inner.link(name, &target.inner).map_err(to_errno)
    }

    /// Move `old` from this directory to `new_dir`/`new`.
    pub fn rename(&self, old: &str, new_dir: &Arc<Inode>, new: &str) -> Result<(), isize> {
        self.inner
            .rename(old, &new_dir.inner, new)
            .map_err(to_errno)
    }
}

/// Map an easy-fs refusal to the kernel errno (docs/vfs.md §11).
fn to_errno(err: VfsError) -> isize {
    match err {
        VfsError::NotFound => ENOENT,
        VfsError::Exists => EEXIST,
        VfsError::NotEmpty => ENOTEMPTY,
        VfsError::NotDirectory => ENOTDIR,
        VfsError::IsDirectory => EISDIR,
        VfsError::InvalidName => EINVAL,
    }
}

/// Split a path into its starting dentry and components, checking the limits.
fn components<'a>(cwd: &Arc<Dentry>, path: &'a str) -> Result<(Arc<Dentry>, Vec<&'a str>), isize> {
    if path.is_empty() {
        return Err(ENOENT);
    }
    if path.len() > PATH_MAX {
        return Err(ENAMETOOLONG);
    }
    let start = if path.starts_with('/') {
        mount::root()
    } else {
        cwd.clone()
    };
    let components: Vec<&str> = path.split('/').filter(|name| !name.is_empty()).collect();
    if components.iter().any(|name| name.len() > NAME_MAX) {
        return Err(ENAMETOOLONG);
    }
    Ok((start, components))
}

/// Walk `components` from `start`; every directory it descends into must grant
/// search permission, and each step must be a directory.
fn walk(mut cur: Arc<Dentry>, components: &[&str], cred: Cred) -> Result<Arc<Dentry>, isize> {
    for name in components {
        if !cur.inode.is_dir() {
            return Err(ENOTDIR);
        }
        permission(&cur.inode, MAY_EXEC, cred)?;
        let inode = cur.inode.find(name).ok_or(ENOENT)?;
        cur = Arc::new(Dentry::new(String::from(*name), inode));
    }
    Ok(cur)
}

/// Resolve `path` against `cwd` as `cred`; the final component must exist.
pub fn lookup(cwd: &Arc<Dentry>, path: &str, cred: Cred) -> Result<Arc<Dentry>, isize> {
    let (start, components) = components(cwd, path)?;
    let dentry = walk(start, &components, cred)?;
    if path.ends_with('/') && !dentry.inode.is_dir() {
        return Err(ENOTDIR);
    }
    Ok(dentry)
}

/// Resolve the parent directory of `path` and the final component name.
pub fn lookup_parent(
    cwd: &Arc<Dentry>,
    path: &str,
    cred: Cred,
) -> Result<(Arc<Dentry>, String), isize> {
    let (start, components) = components(cwd, path)?;
    let Some((name, parents)) = components.split_last() else {
        return Err(ENOENT);
    };
    Ok((walk(start, parents, cred)?, String::from(*name)))
}

/// Open `path` with Linux flags; the returned file keeps its own offset.
///
/// `mode` is the permission word a file created by `O_CREAT` gets, already
/// masked by the caller's umask (docs/perm.md §7).
pub fn open(
    cwd: &Arc<Dentry>,
    path: &str,
    flags: OpenFlags,
    mode: u32,
    cred: Cred,
) -> Result<Arc<OSInode>, isize> {
    if flags.bits() & !OpenFlags::all().bits() != 0 || flags.bits() & O_ACCMODE == O_ACCMODE {
        return Err(EINVAL);
    }
    let (readable, writable) = flags.read_write();
    let append = flags.contains(OpenFlags::APPEND);
    match lookup(cwd, path, cred) {
        Ok(dentry) => {
            let inode = dentry.inode().clone();
            if inode.is_dir() {
                if flags.contains(OpenFlags::CREATE) {
                    if flags.contains(OpenFlags::EXCL) {
                        return Err(EEXIST);
                    }
                    return Err(EISDIR);
                }
                if flags.contains(OpenFlags::TRUNC) || writable {
                    return Err(EISDIR);
                }
                // the descriptor is opened read-only, for getdents
                permission(&inode, MAY_READ, cred)?;
            } else {
                if flags.contains(OpenFlags::CREATE) && flags.contains(OpenFlags::EXCL) {
                    return Err(EEXIST);
                }
                if flags.contains(OpenFlags::DIRECTORY) {
                    return Err(ENOTDIR);
                }
                let mut mask = 0;
                if readable {
                    mask |= MAY_READ;
                }
                if writable {
                    mask |= MAY_WRITE;
                }
                permission(&inode, mask, cred)?;
                // a special file has no driver behind it yet (M3.1, M6.2)
                if inode.mode() & S_IFMT != S_IFREG {
                    return Err(ENXIO);
                }
                if flags.contains(OpenFlags::TRUNC) {
                    inode.truncate();
                }
            }
            Ok(Arc::new(OSInode::new(readable, writable, append, inode)))
        }
        Err(err) if err == ENOENT && flags.contains(OpenFlags::CREATE) && !path.ends_with('/') => {
            // a directory open cannot create the regular file the flag combination asks for
            if flags.contains(OpenFlags::DIRECTORY) {
                return Err(ENOTDIR);
            }
            let (parent, name) = lookup_parent(cwd, path, cred)?;
            permission(parent.inode(), MAY_WRITE | MAY_EXEC, cred)?;
            let inode = parent.inode().create_file(&name, mode, cred)?;
            Ok(Arc::new(OSInode::new(readable, writable, append, inode)))
        }
        Err(err) => Err(err),
    }
}

/// Absolute path of a directory, for getcwd.
pub fn absolute_path(dentry: &Arc<Dentry>) -> Result<String, isize> {
    if !dentry.inode.is_dir() {
        return Err(ENOTDIR);
    }
    let mut names = Vec::new();
    let mut cur = dentry.inode.clone();
    loop {
        // the root's ".." names the root itself, which ends the walk
        let parent = cur.find("..").ok_or(ENOENT)?;
        if parent.inode_id() == cur.inode_id() {
            break;
        }
        names.push(entry_name(&parent, cur.inode_id())?);
        cur = parent;
    }
    let mut path = String::new();
    for name in names.iter().rev() {
        path.push('/');
        path.push_str(name);
    }
    if path.is_empty() {
        path.push('/');
    }
    Ok(path)
}

/// Name of the entry in `dir` that names the inode `inode_id`.
fn entry_name(dir: &Arc<Inode>, inode_id: u32) -> Result<String, isize> {
    let mut offset = 0;
    while let Some(dirent) = dir.dirent_at(offset) {
        if dirent.inode_id == inode_id && dirent.name != "." && dirent.name != ".." {
            return Ok(dirent.name);
        }
        offset = dirent.next_offset;
    }
    Err(ENOENT)
}

/// Whole-file read, for exec and spawn.
pub fn read_all(inode: &Arc<Inode>) -> Vec<u8> {
    let mut data = Vec::new();
    let mut buf = [0u8; 512];
    let mut offset = 0;
    loop {
        let len = inode.read_at(offset, &mut buf);
        if len == 0 {
            break;
        }
        data.extend_from_slice(&buf[..len]);
        offset += len;
    }
    data
}

/// The Linux riscv64 `struct stat` of an inode (docs/perm.md §8).
pub fn stat(inode: &Arc<Inode>) -> Stat {
    let size = inode.size();
    Stat {
        st_dev: mount::dev(),
        st_ino: inode.inode_id() as u64,
        st_mode: inode.mode(),
        st_nlink: inode.nlink(),
        st_uid: inode.uid(),
        st_gid: inode.gid(),
        st_rdev: inode.dev() as u64,
        st_size: size as i64,
        st_blksize: 512,
        st_blocks: (size as i64 + 511) / 512,
        ..Default::default()
    }
}
