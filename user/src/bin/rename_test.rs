// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    EINVAL, ENOENT, OpenFlags, S_IFDIR, S_IFMT, Stat, chdir, close, creat, errno, getcwd, mkdir,
    open, read, rename, rmdir, stat, unlink, write,
};

const NAME: &str = "rename_test";

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

    // Each new top-level directory adds one to the root's nlink.
    t.check("stat /", stat("/\0", &mut st) == 0);
    let root_nlink = st.nlink;
    t.check("mkdir rt_a", mkdir("rt_a\0", 0o777) == 0);
    t.check("mkdir rt_b", mkdir("rt_b\0", 0o777) == 0);
    t.check(
        "root nlink +2",
        stat("/\0", &mut st) == 0 && st.nlink == root_nlink + 2,
    );

    let fd = creat("rt_a/f\0", 0o666);
    t.check("write rt_a/f", write(fd as usize, b"one") == 3);
    close(fd as usize);

    // Within one directory.
    t.check("rename within", rename("rt_a/f\0", "rt_a/g\0") == 0);
    t.check(
        "old name gone",
        failed(open("rt_a/f\0", OpenFlags::RDONLY, 0), ENOENT),
    );
    let mut buf = [0u8; 8];
    let fd = open("rt_a/g\0", OpenFlags::RDONLY, 0);
    t.check(
        "data moved",
        read(fd as usize, &mut buf) == 3 && &buf[..3] == b"one",
    );
    close(fd as usize);

    // Across directories.
    t.check("rename across", rename("rt_a/g\0", "rt_b/g\0") == 0);
    t.check("rt_a/g gone", failed(stat("rt_a/g\0", &mut st), ENOENT));
    let fd = open("rt_b/g\0", OpenFlags::RDONLY, 0);
    t.check(
        "data across",
        read(fd as usize, &mut buf) == 3 && &buf[..3] == b"one",
    );
    close(fd as usize);

    // A file target that exists is replaced: the new inode's size wins.
    let fd = creat("rt_b/target\0", 0o666);
    t.check(
        "write rt_b/target",
        write(fd as usize, b"old content!") == 12,
    );
    close(fd as usize);
    t.check("rename replaces", rename("rt_b/g\0", "rt_b/target\0") == 0);
    t.check("rt_b/g gone", failed(stat("rt_b/g\0", &mut st), ENOENT));
    t.check("stat replaced", stat("rt_b/target\0", &mut st) == 0);
    t.check("replaced size 3", st.size == 3);
    let fd = open("rt_b/target\0", OpenFlags::RDONLY, 0);
    t.check(
        "replaced data",
        read(fd as usize, &mut buf) == 3 && &buf[..3] == b"one",
    );
    close(fd as usize);

    // A directory cannot move into its own subtree.
    t.check("mkdir rt_b/d", mkdir("rt_b/d\0", 0o777) == 0);
    t.check(
        "move into subtree EINVAL",
        failed(rename("rt_b/d\0", "rt_b/d/x\0"), EINVAL),
    );
    t.check(
        "move parent into subtree EINVAL",
        failed(rename("rt_b\0", "rt_b/d/rt_b2\0"), EINVAL),
    );

    // A missing source is ENOENT.
    t.check(
        "rename missing ENOENT",
        failed(rename("rt_a/none\0", "rt_b/none\0"), ENOENT),
    );

    // A directory moves across directories and rewrites its '..' entry.
    t.check("mkdir rt_a/d2", mkdir("rt_a/d2\0", 0o777) == 0);
    t.check(
        "rename a directory across",
        rename("rt_a/d2\0", "rt_b/d2\0") == 0,
    );
    t.check("rt_a/d2 gone", failed(stat("rt_a/d2\0", &mut st), ENOENT));
    t.check("stat rt_b/d2", stat("rt_b/d2\0", &mut st) == 0);
    t.check("rt_b/d2 is a directory", st.mode & S_IFMT == S_IFDIR);
    t.check("chdir rt_b/d2", chdir("rt_b/d2\0") == 0);
    let mut cwd = [0u8; 16];
    t.check(
        "dotdot rewritten",
        getcwd(&mut cwd) == 9 && &cwd[..9] == b"/rt_b/d2\0",
    );
    t.check("chdir /", chdir("/\0") == 0);

    // A directory's nlink is 2 plus one per subdirectory.
    t.check(
        "rt_a nlink 2",
        stat("rt_a\0", &mut st) == 0 && st.nlink == 2,
    );
    t.check(
        "rt_b nlink 4",
        stat("rt_b\0", &mut st) == 0 && st.nlink == 4,
    );

    // Clean up.
    t.check("unlink rt_b/target", unlink("rt_b/target\0") == 0);
    t.check("rmdir rt_b/d", rmdir("rt_b/d\0") == 0);
    t.check("rmdir rt_b/d2", rmdir("rt_b/d2\0") == 0);
    t.check("rmdir rt_b", rmdir("rt_b\0") == 0);
    t.check("rmdir rt_a", rmdir("rt_a\0") == 0);
    t.result()
}
