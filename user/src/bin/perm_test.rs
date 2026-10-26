// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    EACCES, EINVAL, ENOENT, F_OK, OpenFlags, R_OK, S_IFDIR, S_IFREG, Stat, W_OK, X_OK, access,
    chmod, chown, close, creat, errno, exec, mkdir, open, rmdir, stat, umask, unlink,
};

const NAME: &str = "perm_test";

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

    // The boot umask, and the permission bits creat applies under it.
    t.check("boot umask 0o022", umask(0o022) == 0o022);
    let fd = creat("pm_a\0", 0o666);
    t.check("create pm_a", fd >= 0);
    close(fd as usize);
    t.check("stat pm_a", stat("pm_a\0", &mut st) == 0);
    t.check("pm_a is S_IFREG|0o644", st.mode == S_IFREG | 0o644);

    // umask(0o077) masks a created file and directory.
    t.check("umask(0o077) returns 0o022", umask(0o077) == 0o022);
    let fd = creat("pm_b\0", 0o666);
    t.check("create pm_b", fd >= 0);
    close(fd as usize);
    t.check("stat pm_b", stat("pm_b\0", &mut st) == 0);
    t.check("pm_b is S_IFREG|0o600", st.mode == S_IFREG | 0o600);
    t.check("mkdir pm_d", mkdir("pm_d\0", 0o777) == 0);
    t.check("stat pm_d", stat("pm_d\0", &mut st) == 0);
    t.check("pm_d is S_IFDIR|0o700", st.mode == S_IFDIR | 0o700);
    t.check("umask(0o022) returns 0o077", umask(0o022) == 0o077);

    // The caller is root, so access holds on every requested bit.
    t.check("access F_OK", access("pm_a\0", F_OK) == 0);
    t.check("access R_OK", access("pm_a\0", R_OK) == 0);
    t.check("access W_OK", access("pm_a\0", W_OK) == 0);
    t.check("access X_OK", access("pm_a\0", X_OK) == 0);
    t.check(
        "access missing EACCES",
        failed(access("pm_none\0", F_OK), EACCES),
    );
    t.check("access mode 8 EINVAL", failed(access("pm_a\0", 8), EINVAL));

    // chmod rewrites the permission bits and keeps the type, even when the
    // argument itself carries type bits.
    t.check("chmod pm_a 0o600", chmod("pm_a\0", 0o600) == 0);
    t.check("stat pm_a", stat("pm_a\0", &mut st) == 0);
    t.check("pm_a is S_IFREG|0o600", st.mode == S_IFREG | 0o600);
    t.check(
        "chmod pm_a carrying S_IFDIR",
        chmod("pm_a\0", S_IFDIR | 0o640) == 0,
    );
    t.check("stat pm_a", stat("pm_a\0", &mut st) == 0);
    t.check("pm_a keeps S_IFREG|0o640", st.mode == S_IFREG | 0o640);

    // chown sets both halves; u32::MAX leaves one unchanged.
    t.check("chown pm_a 7:9", chown("pm_a\0", 7, 9) == 0);
    t.check("stat pm_a", stat("pm_a\0", &mut st) == 0);
    t.check("pm_a uid 7", st.uid == 7);
    t.check("pm_a gid 9", st.gid == 9);
    t.check("chown pm_a -:11", chown("pm_a\0", u32::MAX, 11) == 0);
    t.check("stat pm_a", stat("pm_a\0", &mut st) == 0);
    t.check("pm_a uid stays 7", st.uid == 7);
    t.check("pm_a gid 11", st.gid == 11);

    // The root bypass: mode 0 still opens for reading and passes access(R_OK).
    let fd = creat("pm_zero\0", 0o666);
    t.check("create pm_zero", fd >= 0);
    close(fd as usize);
    t.check("chmod pm_zero 0", chmod("pm_zero\0", 0) == 0);
    let fd = open("pm_zero\0", OpenFlags::RDONLY, 0);
    t.check("open pm_zero (mode 0)", fd >= 0);
    close(fd as usize);
    t.check("access pm_zero R_OK", access("pm_zero\0", R_OK) == 0);

    // Metadata errors.
    t.check(
        "chmod missing ENOENT",
        failed(chmod("pm_none\0", 0o644), ENOENT),
    );
    t.check(
        "chown missing ENOENT",
        failed(chown("pm_none\0", 0, 0), ENOENT),
    );

    // exec of a non-regular file is EACCES, not ENOEXEC.
    t.check(
        "exec / EACCES",
        failed(exec("/\0", &[core::ptr::null::<u8>()]), EACCES),
    );

    // Clean up; pm_a is chowned, but unlink only needs the parent.
    t.check("unlink pm_a", unlink("pm_a\0") == 0);
    t.check("unlink pm_b", unlink("pm_b\0") == 0);
    t.check("unlink pm_zero", unlink("pm_zero\0") == 0);
    t.check("rmdir pm_d", rmdir("pm_d\0") == 0);
    t.result()
}
