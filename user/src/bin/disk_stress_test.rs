// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

extern crate alloc;

use alloc::format;
use user_lib::{
    ENOENT, OpenFlags, Stat, close, creat, errno, mkdir, open, read, rmdir, stat, unlink, write,
};

const NAME: &str = "disk_stress_test";

/// Subdirectories of the tree under `ds`.
const DIRS: [&str; 4] = ["a", "b", "c", "d"];
/// Files per subdirectory.
const PER_DIR: usize = 25;
/// The block size, and hence the granule of every write and read below.
const BLOCK: usize = 512;

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

/// Run `f` for every file of the tree, as `(index, NUL-terminated path)`.
fn each_file(mut f: impl FnMut(usize, &str)) {
    for (d, dir) in DIRS.iter().enumerate() {
        for k in 0..PER_DIR {
            let i = d * PER_DIR + k;
            f(i, &format!("ds/{}/f{:02}\0", dir, i % PER_DIR));
        }
    }
}

/// A case label: the path without the NUL the syscall wrappers need.
fn plain(path: &str) -> &str {
    path.trim_end_matches('\0')
}

/// File `i`'s size: 2 KiB to 4 KiB, so one file spans four to eight blocks.
fn size_of(i: usize) -> usize {
    2048 + (i % 5) * BLOCK
}

/// The checkable content byte of file `i` at offset `off`.
fn byte_at(i: usize, off: usize) -> u8 {
    ((i * 197 + off * 41) % 251) as u8
}

/// Write file `i`'s pattern through `fd`; false when a write is short.
fn fill(fd: usize, i: usize) -> bool {
    let size = size_of(i);
    let mut buf = [0u8; BLOCK];
    let mut off = 0;
    while off < size {
        let n = (size - off).min(BLOCK);
        for (k, byte) in buf[..n].iter_mut().enumerate() {
            *byte = byte_at(i, off + k);
        }
        if write(fd, &buf[..n]) != n as isize {
            return false;
        }
        off += n;
    }
    true
}

/// Read `path` back and compare it byte for byte; false on a short read, a
/// wrong byte, or a byte past the end of the file.
fn verify(path: &str, i: usize) -> bool {
    let size = size_of(i);
    let fd = open(path, OpenFlags::RDONLY, 0);
    if fd < 0 {
        return false;
    }
    let fd = fd as usize;
    let mut buf = [0u8; BLOCK];
    let mut ok = true;
    let mut off = 0;
    while ok && off < size {
        let n = (size - off).min(BLOCK);
        ok = read(fd, &mut buf[..n]) == n as isize
            && buf[..n]
                .iter()
                .enumerate()
                .all(|(k, &b)| b == byte_at(i, off + k));
        off += n;
    }
    ok = ok && read(fd, &mut buf[..1]) == 0;
    close(fd);
    ok
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut t = Tally::new();

    // The tree: ds/ with four subdirectories of 25 files each.
    t.check("mkdir ds", mkdir("ds\0", 0o777) == 0);
    for dir in DIRS {
        let path = format!("ds/{dir}\0");
        t.check(&format!("mkdir {}", plain(&path)), mkdir(&path, 0o777) == 0);
    }

    // 100 files of 2-4 KiB: 600 blocks of file data, nine times the 64 caches,
    // so a write past the first 64 blocks evicts and writes back a dirty block.
    each_file(|i, path| {
        let fd = creat(path, 0o666);
        t.check(&format!("create {}", plain(path)), fd >= 0);
        t.check(
            &format!("write {}", plain(path)),
            fd >= 0 && fill(fd as usize, i),
        );
        if fd >= 0 {
            close(fd as usize);
        }
    });

    // Read everything back from the disk, byte for byte.
    each_file(|i, path| t.check(&format!("read {}", plain(path)), verify(path, i)));

    // Unlink half of the files: the even-numbered ones.
    each_file(|i, path| {
        if i % 2 == 0 {
            t.check(&format!("unlink {}", plain(path)), unlink(path) == 0);
        }
    });

    // The survivors still read back after the unlink pass, and the unlinked
    // half is unreachable.
    let mut st = Stat::default();
    each_file(|i, path| {
        if i % 2 == 0 {
            t.check(
                &format!("stat {} ENOENT", plain(path)),
                stat(path, &mut st) == -1 && errno() == ENOENT,
            );
        } else {
            t.check(&format!("reread {}", plain(path)), verify(path, i));
        }
    });

    // Remove the tree bottom-up: the survivors, then the directories.
    each_file(|i, path| {
        if i % 2 != 0 {
            t.check(&format!("unlink {}", plain(path)), unlink(path) == 0);
        }
    });
    for dir in DIRS {
        let path = format!("ds/{dir}\0");
        t.check(&format!("rmdir {}", plain(&path)), rmdir(&path) == 0);
    }
    t.check("rmdir ds", rmdir("ds\0") == 0);
    t.check(
        "ds is gone",
        stat("ds\0", &mut st) == -1 && errno() == ENOENT,
    );

    t.result()
}
