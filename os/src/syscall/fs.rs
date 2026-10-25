// SPDX-License-Identifier: GPL-3.0-only
use crate::errno::{EBADF, EINVAL, EISDIR, ENOTDIR, ERANGE};
use crate::fs::{
    Dentry, File, FileDescriptor, OpenFlags, absolute_path, lookup, lookup_parent, make_pipe, open,
    stat,
};
use crate::mm::{copy_to_user, translated_byte_buffer, translated_byte_buffer_mut, translated_str};
use crate::task::{current_process, current_user_token};
use alloc::sync::Arc;
use core::mem::size_of_val;

/// `unlinkat` flag: remove a directory instead of a file (Linux value).
const AT_REMOVEDIR: u32 = 0x200;

/// The calling process's working directory.
fn current_cwd() -> Arc<Dentry> {
    current_process().inner_exclusive_access().cwd.clone()
}

/// The open file behind `fd`, cloned so the process lock is released before the
/// call may block.
fn file_of(fd: usize) -> Result<Arc<dyn File>, isize> {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    match inner.fd_table.get(fd) {
        Some(Some(descriptor)) => Ok(Arc::clone(&descriptor.file)),
        _ => Err(EBADF),
    }
}

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let file = match file_of(fd) {
        Ok(file) => file,
        Err(e) => return e,
    };
    if !file.writable() {
        return EBADF;
    }
    let buffer = match translated_byte_buffer(token, buf, len) {
        Ok(buffer) => buffer,
        Err(e) => return e,
    };
    match file.write(buffer) {
        Ok(len) => len as isize,
        Err(e) => e,
    }
}

pub fn sys_read(fd: usize, buf: *mut u8, len: usize) -> isize {
    let token = current_user_token();
    let file = match file_of(fd) {
        Ok(file) => file,
        Err(e) => return e,
    };
    if !file.readable() {
        return EBADF;
    }
    let buffer = match translated_byte_buffer_mut(token, buf, len) {
        Ok(buffer) => buffer,
        Err(e) => return e,
    };
    match file.read(buffer) {
        Ok(len) => len as isize,
        Err(e) => e,
    }
}

pub fn sys_open(path: *const u8, flags: u32, mode: u32) -> isize {
    let path = match translated_str(current_user_token(), path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let Some(flags) = OpenFlags::from_bits(flags) else {
        return EINVAL;
    };
    let cwd = current_cwd();
    let file = match open(&cwd, &path, flags, mode) {
        Ok(file) => file,
        Err(e) => return e,
    };
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let fd = inner.alloc_fd();
    inner.fd_table[fd] = Some(FileDescriptor::new(
        file,
        flags.contains(OpenFlags::CLOEXEC),
    ));
    fd as isize
}

pub fn sys_close(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let Some(slot) = inner.fd_table.get_mut(fd) else {
        return EBADF;
    };
    if slot.take().is_none() {
        return EBADF;
    }
    0
}

pub fn sys_pipe(pipe: *mut usize) -> isize {
    let token = current_user_token();
    let (pipe_read, pipe_write) = make_pipe();
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(FileDescriptor::new(pipe_read, false));
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(FileDescriptor::new(pipe_write, false));
    let result = copy_to_user(token, pipe as *mut u8, &read_fd.to_ne_bytes()).and_then(|()| {
        copy_to_user(
            token,
            pipe.wrapping_add(1) as *mut u8,
            &write_fd.to_ne_bytes(),
        )
    });
    if let Err(e) = result {
        // a failed pipe must not leave the two descriptors allocated
        inner.fd_table[read_fd] = None;
        inner.fd_table[write_fd] = None;
        return e;
    }
    0
}

pub fn sys_dup(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let file = match inner.fd_table.get(fd).and_then(|slot| slot.as_ref()) {
        Some(descriptor) => Arc::clone(&descriptor.file),
        None => return EBADF,
    };
    let new_fd = inner.alloc_fd();
    inner.fd_table[new_fd] = Some(FileDescriptor::new(file, false));
    new_fd as isize
}

/// Make `newfd` refer to the open file of `oldfd` (docs/reland.md §5).
pub fn sys_dup2(oldfd: usize, newfd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let file = match inner.fd_table.get(oldfd).and_then(|slot| slot.as_ref()) {
        Some(descriptor) => Arc::clone(&descriptor.file),
        None => return EBADF,
    };
    if oldfd == newfd {
        return newfd as isize;
    }
    // The table grows by exactly one slot; skipping ahead would allocate an
    // unbounded hole from a user argument.
    if newfd > inner.fd_table.len() {
        return EBADF;
    }
    // The copy is a new descriptor, so it is not closed by `exec`.
    let descriptor = FileDescriptor::new(file, false);
    if newfd == inner.fd_table.len() {
        inner.fd_table.push(Some(descriptor));
    } else {
        // replacing the slot drops the previous file, which closes it
        inner.fd_table[newfd] = Some(descriptor);
    }
    newfd as isize
}

/// Write the absolute path of the working directory plus its NUL into `buf`.
pub fn sys_getcwd(buf: *mut u8, size: usize) -> isize {
    let cwd = current_cwd();
    let path = match absolute_path(&cwd) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let mut bytes = path.into_bytes();
    bytes.push(0);
    if bytes.len() > size {
        return ERANGE;
    }
    match copy_to_user(current_user_token(), buf, &bytes) {
        Ok(()) => bytes.len() as isize,
        Err(e) => e,
    }
}

pub fn sys_chdir(path: *const u8) -> isize {
    let path = match translated_str(current_user_token(), path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let cwd = current_cwd();
    let dentry = match lookup(&cwd, &path) {
        Ok(dentry) => dentry,
        Err(e) => return e,
    };
    if !dentry.inode().is_dir() {
        return ENOTDIR;
    }
    current_process().inner_exclusive_access().cwd = dentry;
    0
}

pub fn sys_mkdir(path: *const u8, _mode: u32) -> isize {
    let path = match translated_str(current_user_token(), path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let cwd = current_cwd();
    let (parent, name) = match lookup_parent(&cwd, &path) {
        Ok(parent) => parent,
        Err(e) => return e,
    };
    match parent.inode().create_dir(&name) {
        Ok(_) => 0,
        Err(e) => e,
    }
}

/// `unlink`, and `rmdir` through its `AT_REMOVEDIR` flag.
pub fn sys_unlink(path: *const u8, flags: u32) -> isize {
    let path = match translated_str(current_user_token(), path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    if flags & !AT_REMOVEDIR != 0 {
        return EINVAL;
    }
    let cwd = current_cwd();
    let target = match lookup(&cwd, &path) {
        Ok(target) => target,
        Err(e) => return e,
    };
    let (parent, name) = match lookup_parent(&cwd, &path) {
        Ok(parent) => parent,
        Err(e) => return e,
    };
    if flags == AT_REMOVEDIR {
        if !target.inode().is_dir() {
            return ENOTDIR;
        }
    } else if target.inode().is_dir() {
        return EISDIR;
    }
    match parent.inode().remove(&name) {
        Ok(()) => 0,
        Err(e) => e,
    }
}

pub fn sys_link(oldpath: *const u8, newpath: *const u8) -> isize {
    let token = current_user_token();
    let oldpath = match translated_str(token, oldpath) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let newpath = match translated_str(token, newpath) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let cwd = current_cwd();
    let target = match lookup(&cwd, &oldpath) {
        Ok(target) => target,
        Err(e) => return e,
    };
    let (parent, name) = match lookup_parent(&cwd, &newpath) {
        Ok(parent) => parent,
        Err(e) => return e,
    };
    match parent.inode().link(&name, target.inode()) {
        Ok(()) => 0,
        Err(e) => e,
    }
}

pub fn sys_rename(oldpath: *const u8, newpath: *const u8) -> isize {
    let token = current_user_token();
    let oldpath = match translated_str(token, oldpath) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let newpath = match translated_str(token, newpath) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let cwd = current_cwd();
    // The old path must exist, and a trailing slash must name a directory.
    if let Err(e) = lookup(&cwd, &oldpath) {
        return e;
    }
    let (old_parent, old_name) = match lookup_parent(&cwd, &oldpath) {
        Ok(old) => old,
        Err(e) => return e,
    };
    let (new_parent, new_name) = match lookup_parent(&cwd, &newpath) {
        Ok(new) => new,
        Err(e) => return e,
    };
    match old_parent
        .inode()
        .rename(&old_name, new_parent.inode(), &new_name)
    {
        Ok(()) => 0,
        Err(e) => e,
    }
}

pub fn sys_getdents64(fd: usize, buf: *mut u8, len: usize) -> isize {
    let token = current_user_token();
    let file = match file_of(fd) {
        Ok(file) => file,
        Err(e) => return e,
    };
    let buffer = match translated_byte_buffer_mut(token, buf, len) {
        Ok(buffer) => buffer,
        Err(e) => return e,
    };
    match file.getdents(buffer) {
        Ok(len) => len as isize,
        Err(e) => e,
    }
}

pub fn sys_lseek(fd: usize, offset: isize, whence: u32) -> isize {
    let file = match file_of(fd) {
        Ok(file) => file,
        Err(e) => return e,
    };
    match file.lseek(offset, whence) {
        Ok(offset) => offset as isize,
        Err(e) => e,
    }
}

pub fn sys_stat(path: *const u8, buf: *mut u8) -> isize {
    let path = match translated_str(current_user_token(), path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let cwd = current_cwd();
    let dentry = match lookup(&cwd, &path) {
        Ok(dentry) => dentry,
        Err(e) => return e,
    };
    write_stat(buf, &stat(dentry.inode()))
}

pub fn sys_fstat(fd: usize, buf: *mut u8) -> isize {
    let file = match file_of(fd) {
        Ok(file) => file,
        Err(e) => return e,
    };
    match file.stat() {
        Ok(st) => write_stat(buf, &st),
        Err(e) => e,
    }
}

/// Copy the kernel `struct stat` value into the user buffer at `buf`.
fn write_stat<T>(buf: *mut u8, st: &T) -> isize {
    let bytes =
        unsafe { core::slice::from_raw_parts(st as *const T as *const u8, size_of_val(st)) };
    match copy_to_user(current_user_token(), buf, bytes) {
        Ok(()) => 0,
        Err(e) => e,
    }
}
