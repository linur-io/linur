// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    ENOENT, ENOTDIR, ERANGE, OpenFlags, chdir, close, errno, exit, fork, getcwd, mkdir, open,
    rmdir, unlink, waitpid,
};

const NAME: &str = "path_test";

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

    // A small tree: pt_dir/sub/f, reached from the root and from below.
    t.check("mkdir pt_dir", mkdir("pt_dir\0", 0o777) == 0);
    t.check("mkdir pt_dir/sub", mkdir("pt_dir/sub\0", 0o777) == 0);
    let file = "pt_dir/sub/f\0";
    let fd = open(file, OpenFlags::CREATE | OpenFlags::WRONLY, 0o666);
    t.check("create pt_dir/sub/f", fd >= 0);
    close(fd as usize);

    // Absolute, relative, and repeated '/' ignored.
    let fd = open("/pt_dir/sub/f\0", OpenFlags::RDONLY, 0);
    t.check("absolute path", fd >= 0);
    close(fd as usize);
    let fd = open("//pt_dir///sub//f\0", OpenFlags::RDONLY, 0);
    t.check("repeated slashes", fd >= 0);
    close(fd as usize);
    let fd = open(file, OpenFlags::RDONLY, 0);
    t.check("relative path", fd >= 0);
    close(fd as usize);

    // chdir and getcwd; the returned length includes the NUL.
    t.check("chdir pt_dir", chdir("pt_dir\0") == 0);
    let mut buf = [0u8; 16];
    t.check(
        "getcwd pt_dir",
        getcwd(&mut buf) == 8 && &buf[..8] == b"/pt_dir\0",
    );
    t.check("chdir pt_dir/sub", chdir("sub\0") == 0);
    t.check(
        "getcwd pt_dir/sub",
        getcwd(&mut buf) == 12 && &buf[..12] == b"/pt_dir/sub\0",
    );
    let fd = open("f\0", OpenFlags::RDONLY, 0);
    t.check("relative to cwd", fd >= 0);
    close(fd as usize);

    // '.' and '..' resolve through the real entries.
    let fd = open(".\0", OpenFlags::RDONLY | OpenFlags::DIRECTORY, 0);
    t.check("open .", fd >= 0);
    close(fd as usize);
    let fd = open("./f\0", OpenFlags::RDONLY, 0);
    t.check("open ./f", fd >= 0);
    close(fd as usize);
    t.check("chdir ..", chdir("..\0") == 0);
    t.check(
        "getcwd after ..",
        getcwd(&mut buf) == 8 && &buf[..8] == b"/pt_dir\0",
    );
    t.check("chdir ../pt_dir/sub", chdir("../pt_dir/sub\0") == 0);
    t.check(
        "getcwd after .. path",
        getcwd(&mut buf) == 12 && &buf[..12] == b"/pt_dir/sub\0",
    );

    // A too-small getcwd buffer is ERANGE; the cwd is inherited on fork.
    let mut small = [0u8; 4];
    t.check("getcwd ERANGE", failed(getcwd(&mut small), ERANGE));
    let pid = fork();
    if pid == 0 {
        let mut cwd = [0u8; 16];
        let ok = getcwd(&mut cwd) == 12 && &cwd[..12] == b"/pt_dir/sub\0";
        exit(if ok { 0 } else { 1 });
    }
    let mut code = 0;
    let reaped = waitpid(pid as usize, &mut code);
    t.check("cwd inherited on fork", reaped == pid && code == 0);

    // ENOENT and ENOTDIR from the current directory.
    t.check(
        "open missing ENOENT",
        failed(open("no_such\0", OpenFlags::RDONLY, 0), ENOENT),
    );
    t.check("chdir missing ENOENT", failed(chdir("no_such\0"), ENOENT));
    t.check(
        "open through a file ENOTDIR",
        failed(open("f/x\0", OpenFlags::RDONLY, 0), ENOTDIR),
    );
    t.check("chdir a file ENOTDIR", failed(chdir("f\0"), ENOTDIR));
    t.check(
        "trailing slash on a file ENOTDIR",
        failed(open("/pt_dir/sub/f/\0", OpenFlags::RDONLY, 0), ENOTDIR),
    );
    t.check(
        "empty path ENOENT",
        failed(open("\0", OpenFlags::RDONLY, 0), ENOENT),
    );

    // Back to the root and clean up.
    t.check("chdir /", chdir("/\0") == 0);
    t.check("getcwd /", getcwd(&mut buf) == 2 && &buf[..2] == b"/\0");
    t.check("unlink f", unlink("/pt_dir/sub/f\0") == 0);
    t.check("rmdir sub", rmdir("/pt_dir/sub\0") == 0);
    t.check("rmdir pt_dir", rmdir("/pt_dir\0") == 0);
    t.result()
}
