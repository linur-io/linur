// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    EBADF, EINVAL, ESPIPE, OpenFlags, SEEK_CUR, SEEK_END, SEEK_SET, close, creat, errno, lseek,
    open, pipe, read, unlink, write,
};

const NAME: &str = "lseek_test";

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

    let fd = creat("ls_f\0", 0o666);
    t.check("create ls_f", fd >= 0);
    t.check("write abcdefghij", write(fd as usize, b"abcdefghij") == 10);
    close(fd as usize);

    let fd = open("ls_f\0", OpenFlags::RDONLY, 0);
    t.check("open ls_f", fd >= 0);
    t.check("SEEK_SET", lseek(fd as usize, 3, SEEK_SET) == 3);
    let mut buf = [0u8; 2];
    t.check(
        "read after SEEK_SET",
        read(fd as usize, &mut buf) == 2 && &buf[..2] == b"de",
    );
    t.check("SEEK_CUR", lseek(fd as usize, 0, SEEK_CUR) == 5);
    t.check("SEEK_CUR back", lseek(fd as usize, -2, SEEK_CUR) == 3);
    t.check("SEEK_END", lseek(fd as usize, -2, SEEK_END) == 8);
    t.check(
        "read after SEEK_END",
        read(fd as usize, &mut buf) == 2 && &buf[..2] == b"ij",
    );
    t.check("SEEK_END to end", lseek(fd as usize, 0, SEEK_END) == 10);
    let mut byte = [0u8; 1];
    t.check("read at EOF", read(fd as usize, &mut byte) == 0);

    // Errors: a negative result, an unknown whence, a bad fd.
    t.check(
        "negative offset EINVAL",
        failed(lseek(fd as usize, -1, SEEK_SET), EINVAL),
    );
    t.check(
        "unknown whence EINVAL",
        failed(lseek(fd as usize, 0, 99), EINVAL),
    );
    t.check(
        "negative from end EINVAL",
        failed(lseek(fd as usize, -11, SEEK_END), EINVAL),
    );
    t.check("lseek bad fd EBADF", failed(lseek(99, 0, SEEK_SET), EBADF));

    // A pipe and the standard streams have no offset.
    let mut fds = [999usize; 2];
    t.check("pipe", pipe(&mut fds) == 0);
    t.check(
        "lseek a pipe read end ESPIPE",
        failed(lseek(fds[0], 0, SEEK_SET), ESPIPE),
    );
    t.check(
        "lseek a pipe write end ESPIPE",
        failed(lseek(fds[1], 0, SEEK_END), ESPIPE),
    );
    close(fds[0]);
    close(fds[1]);
    t.check("lseek stdout ESPIPE", failed(lseek(1, 0, SEEK_SET), ESPIPE));

    close(fd as usize);
    t.check("unlink ls_f", unlink("ls_f\0") == 0);
    t.result()
}
