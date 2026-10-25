// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    EEXIST, EISDIR, ENOENT, OpenFlags, S_IFMT, S_IFREG, Stat, close, creat, errno, link, mkdir,
    open, read, rmdir, stat, unlink, write,
};

const NAME: &str = "link_test";

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

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut t = Tally::new();

    let fd = creat("lt_a\0", 0o666);
    t.check("create lt_a", fd >= 0);
    t.check("write hello", write(fd as usize, b"hello") == 5);
    close(fd as usize);
    t.check("link lt_a lt_b", link("lt_a\0", "lt_b\0") == 0);

    let mut st = Stat::default();
    t.check("stat lt_a", stat("lt_a\0", &mut st) == 0);
    let ino = st.ino;
    t.check("lt_a nlink 2", st.nlink == 2);
    t.check("lt_a is a regular file", st.mode & S_IFMT == S_IFREG);
    t.check("stat lt_b", stat("lt_b\0", &mut st) == 0);
    t.check("lt_b shares the inode", st.ino == ino);
    t.check("lt_b nlink 2", st.nlink == 2);

    // Both names read the same data; a write through one is seen through the other.
    let mut buf = [0u8; 5];
    let fd = open("lt_b\0", OpenFlags::RDONLY, 0);
    t.check(
        "read through lt_b",
        read(fd as usize, &mut buf) == 5 && buf == *b"hello",
    );
    close(fd as usize);
    let fd = open("lt_b\0", OpenFlags::WRONLY, 0);
    t.check("write through lt_b", write(fd as usize, b"HELLO") == 5);
    close(fd as usize);
    let fd = open("lt_a\0", OpenFlags::RDONLY, 0);
    t.check(
        "read through lt_a",
        read(fd as usize, &mut buf) == 5 && buf == *b"HELLO",
    );
    close(fd as usize);

    // Unlinking one name leaves the inode and the other name.
    t.check("unlink lt_a", unlink("lt_a\0") == 0);
    t.check(
        "lt_a gone",
        failed(open("lt_a\0", OpenFlags::RDONLY, 0), ENOENT),
    );
    t.check("stat lt_b", stat("lt_b\0", &mut st) == 0);
    t.check("lt_b nlink 1", st.nlink == 1);
    let fd = open("lt_b\0", OpenFlags::RDONLY, 0);
    t.check(
        "data survives one name",
        read(fd as usize, &mut buf) == 5 && buf == *b"HELLO",
    );
    close(fd as usize);

    // link and unlink errors.
    t.check(
        "link missing ENOENT",
        failed(link("lt_none\0", "lt_x\0"), ENOENT),
    );
    t.check(
        "link existing EEXIST",
        failed(link("lt_b\0", "lt_b\0"), EEXIST),
    );
    t.check("mkdir lt_dir", mkdir("lt_dir\0", 0o777) == 0);
    t.check(
        "link a directory EISDIR",
        failed(link("lt_dir\0", "lt_dir_link\0"), EISDIR),
    );
    t.check("unlink missing ENOENT", failed(unlink("lt_none\0"), ENOENT));
    t.check("rmdir lt_dir", rmdir("lt_dir\0") == 0);
    t.check("unlink lt_b", unlink("lt_b\0") == 0);
    t.result()
}
