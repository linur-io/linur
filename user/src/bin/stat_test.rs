// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    EBADF, ENOENT, ENOTDIR, OpenFlags, S_IFCHR, S_IFDIR, S_IFIFO, S_IFMT, S_IFREG, Stat, close,
    creat, errno, fstat, link, mkdir, open, pipe, rmdir, stat, unlink, write,
};

const NAME: &str = "stat_test";

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
    let mut st = Stat::default();

    // The root directory.
    t.check("stat /", stat("/\0", &mut st) == 0);
    t.check("/ is a directory", st.mode & S_IFMT == S_IFDIR);
    t.check("/ st_dev 1", st.dev == 1);
    t.check("/ st_blksize 512", st.blksize == 512);

    // An empty regular file, then one with data.
    let fd = creat("st_f\0", 0o666);
    t.check("create st_f", fd >= 0);
    close(fd as usize);
    t.check("stat empty st_f", stat("st_f\0", &mut st) == 0);
    let ino = st.ino;
    t.check("st_f is a regular file", st.mode & S_IFMT == S_IFREG);
    t.check("st_f size 0", st.size == 0);
    t.check("st_f nlink 1", st.nlink == 1);
    t.check("st_f blocks 0", st.blocks == 0);

    let fd = open("st_f\0", OpenFlags::WRONLY, 0);
    t.check("write st_f", write(fd as usize, b"0123456789") == 10);
    close(fd as usize);
    t.check("stat st_f", stat("st_f\0", &mut st) == 0);
    t.check("st_f size 10", st.size == 10);
    t.check("st_f blocks 1", st.blocks == 1);

    // A hard link shares the inode and the nlink count.
    t.check("link st_f st_l", link("st_f\0", "st_l\0") == 0);
    t.check("stat st_l", stat("st_l\0", &mut st) == 0);
    t.check("st_l shares the inode", st.ino == ino);
    t.check("st_l nlink 2", st.nlink == 2);
    t.check(
        "st_f nlink 2",
        stat("st_f\0", &mut st) == 0 && st.nlink == 2,
    );

    // fstat reports the open file's inode like stat does.
    let fd = open("st_f\0", OpenFlags::RDONLY, 0);
    t.check("fstat st_f", fstat(fd as usize, &mut st) == 0);
    t.check("fstat ino", st.ino == ino);
    t.check("fstat size", st.size == 10);
    t.check("fstat nlink", st.nlink == 2);
    t.check("fstat type", st.mode & S_IFMT == S_IFREG);
    close(fd as usize);

    // A directory has type S_IFDIR and nlink 2 (dot plus the parent entry).
    t.check("mkdir st_d", mkdir("st_d\0", 0o777) == 0);
    t.check("stat st_d", stat("st_d\0", &mut st) == 0);
    t.check("st_d is a directory", st.mode & S_IFMT == S_IFDIR);
    t.check("st_d nlink 2", st.nlink == 2);

    // stdout is a character device, a pipe is a FIFO; both have nlink 1.
    t.check("fstat stdout", fstat(1, &mut st) == 0);
    t.check("stdout is a character device", st.mode & S_IFMT == S_IFCHR);
    t.check("stdout nlink 1", st.nlink == 1);
    let mut fds = [999usize; 2];
    t.check("pipe", pipe(&mut fds) == 0);
    t.check("fstat pipe", fstat(fds[0], &mut st) == 0);
    t.check("pipe is a FIFO", st.mode & S_IFMT == S_IFIFO);
    t.check("pipe nlink 1", st.nlink == 1);
    close(fds[0]);
    close(fds[1]);

    // stat and fstat errors.
    t.check(
        "stat missing ENOENT",
        failed(stat("st_none\0", &mut st), ENOENT),
    );
    t.check(
        "stat through a file ENOTDIR",
        failed(stat("st_f/x\0", &mut st), ENOTDIR),
    );
    t.check("fstat bad fd EBADF", failed(fstat(99, &mut st), EBADF));

    t.check("unlink st_l", unlink("st_l\0") == 0);
    t.check("unlink st_f", unlink("st_f\0") == 0);
    t.check("rmdir st_d", rmdir("st_d\0") == 0);
    t.result()
}
