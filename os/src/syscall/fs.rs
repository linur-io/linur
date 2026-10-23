// SPDX-License-Identifier: GPL-3.0-only
use crate::errno::{EBADF, EINVAL, ENAMETOOLONG, ENOENT};
use crate::fs::{OpenFlags, make_pipe, open_file};
use crate::mm::{copy_to_user, translated_byte_buffer, translated_byte_buffer_mut, translated_str};
use crate::task::{current_process, current_user_token};
use alloc::sync::Arc;
use easy_fs::NAME_LENGTH_LIMIT;

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return EBADF;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return EBADF;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        let buffer = match translated_byte_buffer(token, buf, len) {
            Ok(buffer) => buffer,
            Err(e) => return e,
        };
        file.write(buffer) as isize
    } else {
        EBADF
    }
}

pub fn sys_read(fd: usize, buf: *mut u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return EBADF;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return EBADF;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        let buffer = match translated_byte_buffer_mut(token, buf, len) {
            Ok(buffer) => buffer,
            Err(e) => return e,
        };
        file.read(buffer) as isize
    } else {
        EBADF
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let process = current_process();
    let token = current_user_token();
    let path = match translated_str(token, path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let Some(flags) = OpenFlags::from_bits(flags) else {
        return EINVAL;
    };
    if path.is_empty() {
        return ENOENT;
    }
    if path.len() > NAME_LENGTH_LIMIT {
        return ENAMETOOLONG;
    }
    if let Some(inode) = open_file(path.as_str(), flags) {
        let mut inner = process.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        ENOENT
    }
}

pub fn sys_close(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return EBADF;
    }
    if inner.fd_table[fd].is_none() {
        return EBADF;
    }
    inner.fd_table[fd].take();
    0
}

pub fn sys_pipe(pipe: *mut usize) -> isize {
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(pipe_read);
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(pipe_write);
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
    if fd >= inner.fd_table.len() {
        return EBADF;
    }
    if inner.fd_table[fd].is_none() {
        return EBADF;
    }
    let new_fd = inner.alloc_fd();
    inner.fd_table[new_fd] = Some(Arc::clone(inner.fd_table[fd].as_ref().unwrap()));
    new_fd as isize
}

/// Make `newfd` refer to the open file of `oldfd` (docs/reland.md §5).
pub fn sys_dup2(oldfd: usize, newfd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if oldfd >= inner.fd_table.len() {
        return EBADF;
    }
    let Some(file) = &inner.fd_table[oldfd] else {
        return EBADF;
    };
    let file = Arc::clone(file);
    if oldfd == newfd {
        return newfd as isize;
    }
    // The table grows by exactly one slot; skipping ahead would allocate an
    // unbounded hole from a user argument.
    if newfd > inner.fd_table.len() {
        return EBADF;
    }
    if newfd == inner.fd_table.len() {
        inner.fd_table.push(Some(file));
    } else {
        // replacing the slot drops the previous file, which closes it
        inner.fd_table[newfd] = Some(file);
    }
    newfd as isize
}
