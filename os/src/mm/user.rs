// SPDX-License-Identifier: GPL-3.0-only
//! Fault-safe access to user memory.
//!
//! A syscall receives pointers into an address space the kernel does not trust,
//! so every range is validated against the user page table before it is
//! touched: a bad pointer becomes `EFAULT`, never a panic. The kernel takes no
//! page fault on this path, because a range that fails validation is never
//! dereferenced.

use super::{PageTable, StepByOne, VirtAddr};
use crate::config::PAGE_SIZE;
use crate::errno::EFAULT;
use alloc::string::String;
use alloc::vec::Vec;

/// Highest virtual address a user mapping may occupy (lower canonical SV39 half).
const USER_ADDR_TOP: usize = 1 << 38;

/// A run of validated user memory, split at page boundaries.
pub struct UserBuffer {
    pub buffers: Vec<&'static mut [u8]>,
}

impl UserBuffer {
    pub fn new(buffers: Vec<&'static mut [u8]>) -> Self {
        Self { buffers }
    }
    pub fn len(&self) -> usize {
        self.buffers.iter().map(|b| b.len()).sum()
    }
}

impl IntoIterator for UserBuffer {
    type Item = *mut u8;
    type IntoIter = UserBufferIterator;
    fn into_iter(self) -> Self::IntoIter {
        UserBufferIterator {
            buffers: self.buffers,
            current_buffer: 0,
            current_idx: 0,
        }
    }
}

pub struct UserBufferIterator {
    buffers: Vec<&'static mut [u8]>,
    current_buffer: usize,
    current_idx: usize,
}

impl Iterator for UserBufferIterator {
    type Item = *mut u8;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_buffer >= self.buffers.len() {
            None
        } else {
            let r = &mut self.buffers[self.current_buffer][self.current_idx] as *mut _;
            if self.current_idx + 1 == self.buffers[self.current_buffer].len() {
                self.current_idx = 0;
                self.current_buffer += 1;
            } else {
                self.current_idx += 1;
            }
            Some(r)
        }
    }
}

/// Validate `ptr..ptr + len` and split it into page-sized runs. `writable`
/// asks for pages the kernel may write, and is checked with the other
/// permissions before a single byte is touched.
fn user_pages(token: usize, ptr: usize, len: usize, writable: bool) -> Result<UserBuffer, isize> {
    let page_table = PageTable::from_token(token);
    let end = match ptr.checked_add(len) {
        Some(end) if len == 0 || end <= USER_ADDR_TOP => end,
        _ => return Err(EFAULT),
    };
    let mut buffers = Vec::new();
    let mut start = ptr;
    while start < end {
        let start_va = VirtAddr::from(start);
        let pte = page_table.translate(start_va.floor()).ok_or(EFAULT)?;
        if !pte.is_valid() || !pte.user() || (writable && !pte.writable()) {
            return Err(EFAULT);
        }
        let mut next_vpn = start_va.floor();
        next_vpn.step();
        let end_va = VirtAddr::from(next_vpn).min(VirtAddr::from(end));
        let offset = start_va.page_offset();
        let chunk_end = if end_va.page_offset() == 0 {
            PAGE_SIZE
        } else {
            end_va.page_offset()
        };
        buffers.push(&mut pte.ppn().get_bytes_array()[offset..chunk_end]);
        start = end_va.into();
    }
    Ok(UserBuffer::new(buffers))
}

/// User memory the kernel reads, validated page by page.
pub fn translated_byte_buffer(
    token: usize,
    ptr: *const u8,
    len: usize,
) -> Result<UserBuffer, isize> {
    user_pages(token, ptr as usize, len, false)
}

/// User memory the kernel writes, validated page by page.
pub fn translated_byte_buffer_mut(
    token: usize,
    ptr: *mut u8,
    len: usize,
) -> Result<UserBuffer, isize> {
    user_pages(token, ptr as usize, len, true)
}

/// Load a NUL-terminated string out of the user address space of `token`.
pub fn translated_str(token: usize, ptr: *const u8) -> Result<String, isize> {
    let page_table = PageTable::from_token(token);
    let mut string = String::new();
    let mut va = ptr as usize;
    loop {
        if va >= USER_ADDR_TOP {
            return Err(EFAULT);
        }
        let addr = VirtAddr::from(va);
        let pte = page_table.translate(addr.floor()).ok_or(EFAULT)?;
        if !pte.is_valid() || !pte.user() {
            return Err(EFAULT);
        }
        let ch = pte.ppn().get_bytes_array()[addr.page_offset()];
        if ch == 0 {
            return Ok(string);
        }
        string.push(ch as char);
        va += 1;
    }
}

/// Copy `dst.len()` bytes out of the user address space of `token`.
pub fn copy_from_user(token: usize, dst: &mut [u8], src: *const u8) -> Result<(), isize> {
    let mut copied = 0;
    for chunk in user_pages(token, src as usize, dst.len(), false)?.buffers {
        let end = copied + chunk.len();
        dst[copied..end].copy_from_slice(chunk);
        copied = end;
    }
    Ok(())
}

/// Copy `src` into the user address space of `token` at `dst`.
pub fn copy_to_user(token: usize, dst: *mut u8, src: &[u8]) -> Result<(), isize> {
    let mut copied = 0;
    for chunk in user_pages(token, dst as usize, src.len(), true)?.buffers {
        let end = copied + chunk.len();
        chunk.copy_from_slice(&src[copied..end]);
        copied = end;
    }
    Ok(())
}
