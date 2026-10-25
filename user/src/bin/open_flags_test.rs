// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use core::arch::asm;
use user_lib::{
    EEXIST, EINVAL, EISDIR, ENOENT, OpenFlags, SEEK_SET, Stat, close, errno, lseek, mkdir, open,
    read, rmdir, stat, unlink, write,
};

const NAME: &str = "open_flags_test";
const SYSCALL_OPEN: usize = 56;

/// Counts expectations; a failed one prints which case broke, and the exit code
/// is 0 only when every one held.
struct Tally {
    passed: usize,
    total: usize,
}

impl Tally {
    const fn new() -> Self {
        Self {
            passed: 0,
            total: 0,
        }
    }

    fn check(&mut self, case: &str, ok: bool) {
        self.total += 1;
        if ok {
            self.passed += 1;
        } else {
            println!("{}: FAIL {}", NAME, case);
        }
    }

    fn result(&self) -> i32 {
        println!("{}: {}/{} passed", NAME, self.passed, self.total);
        if self.passed == self.total { 0 } else { 1 }
    }
}

/// True when a wrapper returned -1 with 'want' in errno(); call it right after.
fn failed(ret: isize, want: i32) -> bool {
    ret == -1 && errno() == want
}

/// Raw open ecall, for a flag word OpenFlags cannot represent; it returns the
/// kernel's -errno directly.
fn open_raw(path: &str, flags: u32) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") path.as_ptr() as usize => ret,
            in("x11") flags as usize,
            in("x12") 0usize,
            in("x17") SYSCALL_OPEN
        );
    }
    ret
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut t = Tally::new();
    let mut st = Stat::default();

    // O_CREAT|O_EXCL creates once and then refuses.
    let fd = open(
        "of_a\0",
        OpenFlags::CREATE | OpenFlags::EXCL | OpenFlags::WRONLY,
        0o666,
    );
    t.check("O_CREAT|O_EXCL creates", fd >= 0);
    close(fd as usize);
    t.check(
        "O_EXCL on an existing file EEXIST",
        failed(
            open(
                "of_a\0",
                OpenFlags::CREATE | OpenFlags::EXCL | OpenFlags::WRONLY,
                0o666,
            ),
            EEXIST,
        ),
    );

    // O_CREAT no longer truncates (docs/vfs.md §7).
    let fd = open("of_a\0", OpenFlags::WRONLY, 0);
    t.check("write keepme", write(fd as usize, b"keepme") == 6);
    close(fd as usize);
    let fd = open("of_a\0", OpenFlags::CREATE | OpenFlags::WRONLY, 0o666);
    t.check("O_CREAT opens an existing file", fd >= 0);
    close(fd as usize);
    t.check(
        "O_CREAT keeps content",
        stat("of_a\0", &mut st) == 0 && st.size == 6,
    );

    // O_TRUNC empties it; O_APPEND writes at the end whatever the offset is.
    let fd = open("of_a\0", OpenFlags::WRONLY | OpenFlags::TRUNC, 0);
    t.check("O_TRUNC", fd >= 0);
    close(fd as usize);
    t.check(
        "truncated size 0",
        stat("of_a\0", &mut st) == 0 && st.size == 0,
    );
    let fd = open("of_a\0", OpenFlags::WRONLY | OpenFlags::APPEND, 0);
    t.check("write appended abc", write(fd as usize, b"abc") == 3);
    t.check(
        "SEEK_SET before append",
        lseek(fd as usize, 0, SEEK_SET) == 0,
    );
    t.check("write appended def", write(fd as usize, b"def") == 3);
    close(fd as usize);
    let mut buf = [0u8; 8];
    let fd = open("of_a\0", OpenFlags::RDONLY, 0);
    t.check(
        "appended data",
        read(fd as usize, &mut buf) == 6 && &buf[..6] == b"abcdef",
    );
    close(fd as usize);

    // O_RDWR reads and writes through one descriptor.
    let fd = open("of_a\0", OpenFlags::RDWR, 0);
    t.check("O_RDWR", fd >= 0);
    let mut head = [0u8; 3];
    t.check(
        "read abc",
        read(fd as usize, &mut head) == 3 && &head == b"abc",
    );
    t.check("write XYZ at offset 3", write(fd as usize, b"XYZ") == 3);
    close(fd as usize);
    let fd = open("of_a\0", OpenFlags::RDONLY, 0);
    t.check(
        "mixed data",
        read(fd as usize, &mut buf) == 6 && &buf[..6] == b"abcXYZ",
    );
    close(fd as usize);

    // A directory only opens O_RDONLY and without O_TRUNC.
    t.check("mkdir of_d", mkdir("of_d\0", 0o777) == 0);
    t.check(
        "O_CREAT on a directory EISDIR",
        failed(
            open("of_d\0", OpenFlags::CREATE | OpenFlags::WRONLY, 0o666),
            EISDIR,
        ),
    );
    t.check(
        "O_WRONLY on a directory EISDIR",
        failed(open("of_d\0", OpenFlags::WRONLY, 0), EISDIR),
    );
    t.check(
        "O_TRUNC on a directory EISDIR",
        failed(
            open("of_d\0", OpenFlags::RDONLY | OpenFlags::TRUNC, 0),
            EISDIR,
        ),
    );

    // Missing without O_CREAT, and unknown flag bits.
    t.check(
        "missing without O_CREAT ENOENT",
        failed(open("of_none\0", OpenFlags::RDONLY, 0), ENOENT),
    );
    t.check(
        "unknown flag bits EINVAL",
        open_raw("of_a\0", 0x4000_0000) == -(EINVAL as isize),
    );

    t.check("unlink of_a", unlink("of_a\0") == 0);
    t.check("rmdir of_d", rmdir("of_d\0") == 0);
    t.result()
}
