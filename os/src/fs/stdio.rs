// SPDX-License-Identifier: GPL-3.0-only
use super::File;
use super::stat::{S_IFCHR, Stat};
use crate::mm::UserBuffer;
use crate::sbi::console_getchar;
use crate::task::suspend_current_and_run_next;

pub struct Stdin;

pub struct Stdout;

/// `struct stat` of the console character device.
fn chr_stat() -> Stat {
    Stat {
        st_dev: 1,
        st_mode: S_IFCHR | 0o666,
        st_nlink: 1,
        st_blksize: 512,
        ..Default::default()
    }
}

impl File for Stdin {
    fn readable(&self) -> bool {
        true
    }
    fn writable(&self) -> bool {
        false
    }
    fn read(&self, mut user_buf: UserBuffer) -> Result<usize, isize> {
        if user_buf.len() == 0 {
            return Ok(0);
        }
        // busy loop
        let mut c: usize;
        loop {
            c = console_getchar();
            if c == 0 {
                suspend_current_and_run_next();
                continue;
            } else {
                break;
            }
        }
        let ch = c as u8;
        unsafe {
            user_buf.buffers[0].as_mut_ptr().write_volatile(ch);
        }
        Ok(1)
    }
    fn write(&self, _user_buf: UserBuffer) -> Result<usize, isize> {
        panic!("Cannot write to stdin!");
    }
    fn stat(&self) -> Result<Stat, isize> {
        Ok(chr_stat())
    }
}

impl File for Stdout {
    fn readable(&self) -> bool {
        false
    }
    fn writable(&self) -> bool {
        true
    }
    fn read(&self, _user_buf: UserBuffer) -> Result<usize, isize> {
        panic!("Cannot read from stdout!");
    }
    fn write(&self, user_buf: UserBuffer) -> Result<usize, isize> {
        for buffer in user_buf.buffers.iter() {
            for byte in buffer.iter() {
                crate::console::put_byte(*byte);
            }
        }
        Ok(user_buf.len())
    }
    fn stat(&self) -> Result<Stat, isize> {
        Ok(chr_stat())
    }
}
