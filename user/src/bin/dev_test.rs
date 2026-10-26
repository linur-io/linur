// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{
    EEXIST, EINVAL, ENOENT, ENXIO, OpenFlags, S_IFCHR, S_IFDIR, S_IFIFO, S_IFMT, Stat, close,
    dirents, errno, getdents, makedev, mknod, open, stat, umask, unlink,
};

const NAME: &str = "dev_test";

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

    // /dev is a directory of the one filesystem.
    t.check("stat /dev", stat("/dev\0", &mut st) == 0);
    t.check("/dev is a directory", st.mode & S_IFMT == S_IFDIR);

    // The seed nodes report S_IFCHR, mode 0o666 and their device number.
    t.check("stat /dev/null", stat("/dev/null\0", &mut st) == 0);
    t.check("/dev/null is S_IFCHR|0o666", st.mode == S_IFCHR | 0o666);
    t.check(
        "/dev/null rdev makedev(1, 3)",
        st.rdev == makedev(1, 3) as u64,
    );
    t.check("stat /dev/zero", stat("/dev/zero\0", &mut st) == 0);
    t.check("/dev/zero is S_IFCHR|0o666", st.mode == S_IFCHR | 0o666);
    t.check(
        "/dev/zero rdev makedev(1, 5)",
        st.rdev == makedev(1, 5) as u64,
    );
    t.check("stat /dev/console", stat("/dev/console\0", &mut st) == 0);
    t.check("/dev/console is S_IFCHR|0o666", st.mode == S_IFCHR | 0o666);
    t.check(
        "/dev/console rdev makedev(5, 1)",
        st.rdev == makedev(5, 1) as u64,
    );

    // getdents lists exactly the three nodes plus . and ...
    let dir_fd = open("/dev\0", OpenFlags::RDONLY | OpenFlags::DIRECTORY, 0);
    t.check("open /dev", dir_fd >= 0);
    let mut seen = [false; 5]; // ".", "..", "null", "zero", "console"
    let mut records = 0usize;
    let mut ended = false;
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
            records += 1;
            match e.name.as_str() {
                "." => seen[0] = true,
                ".." => seen[1] = true,
                "null" => seen[2] = true,
                "zero" => seen[3] = true,
                "console" => seen[4] = true,
                _ => t.check("unexpected /dev entry", false),
            }
        }
    }
    t.check("getdents /dev ended", ended);
    t.check(
        "getdents /dev lists five entries",
        records == 5 && seen.iter().all(|s| *s),
    );
    close(dir_fd as usize);

    // A device node opens ENXIO for reading and for writing.
    t.check(
        "open /dev/null read ENXIO",
        failed(open("/dev/null\0", OpenFlags::RDONLY, 0), ENXIO),
    );
    t.check(
        "open /dev/null write ENXIO",
        failed(open("/dev/null\0", OpenFlags::WRONLY, 0), ENXIO),
    );

    // mknod: the mode carries the type, umask masks the permission bits.
    umask(0o027);
    t.check(
        "mknod dv_chr",
        mknod("dv_chr\0", S_IFCHR | 0o666, makedev(1, 3)) == 0,
    );
    t.check("stat dv_chr", stat("dv_chr\0", &mut st) == 0);
    t.check("dv_chr is S_IFCHR|0o640", st.mode == S_IFCHR | 0o640);
    t.check("dv_chr rdev makedev(1, 3)", st.rdev == makedev(1, 3) as u64);
    t.check("dv_chr nlink 1", st.nlink == 1);
    umask(0o022);
    t.check(
        "mknod existing EEXIST",
        failed(mknod("dv_chr\0", S_IFCHR | 0o666, 0), EEXIST),
    );
    t.check(
        "mknod missing parent ENOENT",
        failed(mknod("dv_none/dv_x\0", S_IFCHR | 0o666, 0), ENOENT),
    );
    t.check(
        "mknod S_IFDIR EINVAL",
        failed(mknod("dv_dir\0", S_IFDIR | 0o755, 0), EINVAL),
    );
    t.check("mknod dv_fifo", mknod("dv_fifo\0", S_IFIFO | 0o600, 0) == 0);
    t.check("stat dv_fifo", stat("dv_fifo\0", &mut st) == 0);
    t.check("dv_fifo is S_IFIFO", st.mode & S_IFMT == S_IFIFO);
    t.check(
        "open dv_fifo ENXIO",
        failed(open("dv_fifo\0", OpenFlags::RDONLY, 0), ENXIO),
    );

    // Clean up.
    t.check("unlink dv_chr", unlink("dv_chr\0") == 0);
    t.check("unlink dv_fifo", unlink("dv_fifo\0") == 0);
    t.result()
}
