// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    ENOENT, OpenFlags, S_IFDIR, S_IFMT, Stat, close, creat, dirents, errno, getdents, mkdir, open,
    rmdir, stat, unlink,
};

const NAME: &str = "mount_test";

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

    // The root directory, on mount 1.
    t.check("stat /", stat("/\0", &mut st) == 0);
    let root_ino = st.ino;
    t.check("/ is a directory", st.mode & S_IFMT == S_IFDIR);
    t.check("/ st_dev 1", st.dev == 1);

    // Its . and .. entries both name the root inode.
    let dir_fd = open("/\0", OpenFlags::RDONLY | OpenFlags::DIRECTORY, 0);
    t.check("open /", dir_fd >= 0);
    let mut dot = false;
    let mut dotdot = false;
    let mut ended = false;
    let mut entries = [0u8; 1024];
    loop {
        let n = getdents(dir_fd as usize, &mut entries);
        if n == 0 {
            ended = true;
            break;
        }
        if n < 0 {
            break;
        }
        for e in dirents(&entries, n as usize) {
            match e.name.as_str() {
                "." => dot = e.ino == root_ino,
                ".." => dotdot = e.ino == root_ino,
                _ => {}
            }
        }
    }
    t.check("getdents / ended", ended);
    t.check("/ . and .. name the root inode", dot && dotdot);
    close(dir_fd as usize);

    // /dev is not a separate mount: same st_dev, and its .. names /.
    t.check("stat /dev", stat("/dev\0", &mut st) == 0);
    t.check("/dev st_dev 1", st.dev == 1);
    let dir_fd = open("/dev\0", OpenFlags::RDONLY | OpenFlags::DIRECTORY, 0);
    t.check("open /dev", dir_fd >= 0);
    let mut parent = false;
    ended = false;
    let mut entries = [0u8; 256];
    loop {
        let n = getdents(dir_fd as usize, &mut entries);
        if n == 0 {
            ended = true;
            break;
        }
        if n < 0 {
            break;
        }
        for e in dirents(&entries, n as usize) {
            if e.name == ".." {
                parent = e.ino == root_ino;
            }
        }
    }
    t.check("getdents /dev ended", ended);
    t.check("/dev .. names /", parent);
    close(dir_fd as usize);

    // Every file the test creates belongs to the same filesystem.
    t.check("mkdir mt_d", mkdir("mt_d\0", 0o755) == 0);
    let fd = creat("mt_d/f\0", 0o666);
    t.check("create mt_d/f", fd >= 0);
    close(fd as usize);
    t.check("mt_d st_dev 1", stat("mt_d\0", &mut st) == 0 && st.dev == 1);
    t.check(
        "mt_d/f st_dev 1",
        stat("mt_d/f\0", &mut st) == 0 && st.dev == 1,
    );

    // A path under a missing directory is ENOENT.
    t.check(
        "stat under missing dir ENOENT",
        failed(stat("mt_none/f\0", &mut st), ENOENT),
    );

    // Clean up.
    t.check("unlink mt_d/f", unlink("mt_d/f\0") == 0);
    t.check("rmdir mt_d", rmdir("mt_d\0") == 0);
    t.result()
}
