// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{OpenFlags, Stat, close, exec, exit, fork, fstat, open, unlink, waitpid, write};

const NAME: &str = "cloexec_test";

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

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut t = Tally::new();

    // A fresh image has only 0, 1 and 2 open, so these land on 3 and 4.
    let fd = open(
        "ce_file\0",
        OpenFlags::CREATE | OpenFlags::WRONLY | OpenFlags::TRUNC,
        0o666,
    );
    t.check("create ce_file", fd >= 0);
    t.check("write ce_file", write(fd as usize, b"x") == 1);
    close(fd as usize);
    let cloexec_fd = open("ce_file\0", OpenFlags::RDONLY | OpenFlags::CLOEXEC, 0);
    let plain_fd = open("ce_file\0", OpenFlags::RDONLY, 0);
    t.check("O_CLOEXEC fd is 3", cloexec_fd == 3);
    t.check("plain fd is 4", plain_fd == 4);
    let mut st = Stat::default();
    t.check(
        "fstat fd 3 before exec",
        fstat(cloexec_fd as usize, &mut st) == 0,
    );

    // The helper checks fd 3 is EBADF and fd 4 still open after exec.
    let pid = fork();
    if pid == 0 {
        exec("cloexec_child\0", &[core::ptr::null::<u8>()]);
        exit(127);
    }
    let mut code = 0;
    let reaped = waitpid(pid as usize, &mut code);
    t.check(
        "exec closes fd 3 and keeps fd 4",
        reaped == pid && code == 0,
    );

    // exec closes only after the new image is installed: a failed exec changes
    // nothing, so both descriptors are still open.
    let pid = fork();
    if pid == 0 {
        let failed_exec = exec("ce_no_such\0", &[core::ptr::null::<u8>()]) == -1;
        let mut st = Stat::default();
        let open3 = fstat(3, &mut st) == 0;
        let open4 = fstat(4, &mut st) == 0;
        exit(if failed_exec && open3 && open4 { 0 } else { 1 });
    }
    let mut code = 0;
    let reaped = waitpid(pid as usize, &mut code);
    t.check("a failed exec closes nothing", reaped == pid && code == 0);

    // The parent's descriptors are unaffected by the child's exec.
    let mut st = Stat::default();
    t.check("parent fd 3 open", fstat(cloexec_fd as usize, &mut st) == 0);
    t.check("parent fd 4 open", fstat(plain_fd as usize, &mut st) == 0);

    close(cloexec_fd as usize);
    close(plain_fd as usize);
    t.check("unlink ce_file", unlink("ce_file\0") == 0);
    t.result()
}
