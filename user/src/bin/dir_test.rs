// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    DT_DIR, DT_REG, EEXIST, EINVAL, EISDIR, ENOENT, ENOTDIR, ENOTEMPTY, OpenFlags, Stat, close,
    creat, dirents, errno, getdents, mkdir, open, read, rmdir, stat, unlink,
};

const NAME: &str = "dir_test";

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

    // A directory with one subdirectory and one file.
    t.check("mkdir dt_dir", mkdir("dt_dir\0", 0o777) == 0);
    t.check("mkdir dt_dir/sub", mkdir("dt_dir/sub\0", 0o777) == 0);
    let fd = creat("dt_dir/f\0", 0o666);
    t.check("create dt_dir/f", fd >= 0);
    close(fd as usize);

    // mkdir errors.
    t.check(
        "mkdir existing EEXIST",
        failed(mkdir("dt_dir\0", 0o777), EEXIST),
    );
    t.check(
        "mkdir missing parent ENOENT",
        failed(mkdir("dt_none/x\0", 0o777), ENOENT),
    );

    // O_DIRECTORY opens a directory and refuses anything else.
    let dir_fd = open("dt_dir\0", OpenFlags::RDONLY | OpenFlags::DIRECTORY, 0);
    t.check("open O_DIRECTORY", dir_fd >= 0);
    t.check(
        "O_DIRECTORY on a file ENOTDIR",
        failed(
            open("dt_dir/f\0", OpenFlags::RDONLY | OpenFlags::DIRECTORY, 0),
            ENOTDIR,
        ),
    );
    let mut byte = [0u8; 1];
    t.check(
        "read a directory EISDIR",
        failed(read(dir_fd as usize, &mut byte), EISDIR),
    );

    // getdents64 lists . and .. like any other entry; a 32-byte buffer fits one
    // record at a time, so the loop also checks that d_off advances the fd.
    let mut seen = [false; 4]; // ".", "..", "sub", "f"
    let mut dot_ino = 0u64;
    let mut f_ino = 0u64;
    let mut records = 0usize;
    let mut ended = false;
    let mut entries = [0u8; 32];
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
            records += 1;
            match e.name.as_str() {
                "." => {
                    seen[0] = true;
                    dot_ino = e.ino;
                    t.check(". is a directory", e.dtype == DT_DIR);
                }
                ".." => {
                    seen[1] = true;
                    t.check(".. is a directory", e.dtype == DT_DIR);
                }
                "sub" => {
                    seen[2] = true;
                    t.check("sub is a directory", e.dtype == DT_DIR);
                }
                "f" => {
                    seen[3] = true;
                    f_ino = e.ino;
                    t.check("f is a regular file", e.dtype == DT_REG);
                }
                _ => t.check("unexpected entry", false),
            }
        }
    }
    t.check("getdents ended", ended);
    t.check(
        "getdents listed all four entries",
        records == 4 && seen.iter().all(|s| *s),
    );

    // An entry's d_ino is the inode number stat reports.
    let mut st = Stat::default();
    t.check("stat dt_dir", stat("dt_dir\0", &mut st) == 0);
    t.check(". names dt_dir's inode", dot_ino == st.ino);
    t.check("stat dt_dir/f", stat("dt_dir/f\0", &mut st) == 0);
    t.check("f names its inode", f_ino == st.ino);
    close(dir_fd as usize);

    // getdents on a non-directory is ENOTDIR.
    let fd = open("dt_dir/f\0", OpenFlags::RDONLY, 0);
    t.check(
        "getdents on a file ENOTDIR",
        failed(getdents(fd as usize, &mut entries), ENOTDIR),
    );
    close(fd as usize);

    // rmdir refuses a non-empty directory, a plain file and dot.
    t.check(
        "rmdir non-empty ENOTEMPTY",
        failed(rmdir("dt_dir\0"), ENOTEMPTY),
    );
    let fd = creat("dt_plain\0", 0o666);
    close(fd as usize);
    t.check("rmdir a file ENOTDIR", failed(rmdir("dt_plain\0"), ENOTDIR));
    t.check("unlink dt_plain", unlink("dt_plain\0") == 0);
    t.check("mkdir dt_dir2", mkdir("dt_dir2\0", 0o777) == 0);
    t.check(
        "unlink a directory EISDIR",
        failed(unlink("dt_dir2\0"), EISDIR),
    );
    t.check("rmdir dt_dir2", rmdir("dt_dir2\0") == 0);
    t.check("rmdir . EINVAL", failed(rmdir(".\0"), EINVAL));

    // Clean up.
    t.check("unlink dt_dir/f", unlink("dt_dir/f\0") == 0);
    t.check("rmdir dt_dir/sub", rmdir("dt_dir/sub\0") == 0);
    t.check("rmdir dt_dir", rmdir("dt_dir\0") == 0);
    t.result()
}
