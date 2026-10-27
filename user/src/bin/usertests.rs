// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

// Every program in src/bin is in SUCC_TESTS, in FAIL_TESTS, or in this list.
// Not in either suite, and why:
//   count_lines  reads stdin until EOF; exercised by the shell pipeline (M3.3)
//   infloop      never returns; it exists to test ^C once signals land (M3.2)
//   initproc     the init process itself
//   user_shell   interactive by design
//   usertests    this runner
//   ch5_exit0, ch5_exit1, ch5_getpid  spawned helpers of spawn0/spawn1 (M2.1)
//   ch5_stride0-5                     spawned helpers of ch5_stride (M2.1)
//   cloexec_child                     spawned helper of cloexec_test (M2.2)
//   envp_child                        spawned helper of elf_test (M2.5)

// item of TESTS : app_name(argv_0), argv_1, argv_2, argv_3, exit_code
static SUCC_TESTS: &[(&str, &str, &str, &str, i32)] = &[
    ("filetest_simple\0", "\0", "\0", "\0", 0),
    ("cat\0", "filea\0", "\0", "\0", 0),
    ("cmdline_args\0", "1\0", "2\0", "3\0", 0),
    ("exit\0", "\0", "\0", "\0", 0),
    ("fantastic_text\0", "\0", "\0", "\0", 0),
    ("forktest_simple\0", "\0", "\0", "\0", 0),
    ("forktest\0", "\0", "\0", "\0", 0),
    ("forktest2\0", "\0", "\0", "\0", 0),
    ("forktree\0", "\0", "\0", "\0", 0),
    ("hello_world\0", "\0", "\0", "\0", 0),
    ("huge_write\0", "\0", "\0", "\0", 0),
    ("matrix\0", "\0", "\0", "\0", 0),
    ("mpsc_sem\0", "\0", "\0", "\0", 0),
    ("phil_din_mutex\0", "\0", "\0", "\0", 0),
    ("pipe_large_test\0", "\0", "\0", "\0", 0),
    ("pipetest\0", "\0", "\0", "\0", 0),
    ("adder_peterson_spin\0", "\0", "\0", "\0", 0),
    ("adder_peterson_yield\0", "\0", "\0", "\0", 0),
    ("adder_mutex_blocking\0", "\0", "\0", "\0", 0),
    ("adder_mutex_spin\0", "\0", "\0", "\0", 0),
    ("run_pipe_test\0", "\0", "\0", "\0", 0),
    ("sleep_simple\0", "\0", "\0", "\0", 0),
    ("sleep\0", "\0", "\0", "\0", 0),
    ("sync_sem\0", "\0", "\0", "\0", 0),
    ("condsync_sem\0", "\0", "\0", "\0", 0),
    ("condsync_condvar\0", "\0", "\0", "\0", 0),
    ("threads_arg\0", "\0", "\0", "\0", 0),
    ("threads\0", "\0", "\0", "\0", 0),
    ("yield\0", "\0", "\0", "\0", 0),
    ("barrier_fail\0", "\0", "\0", "\0", 0),
    ("barrier_condvar\0", "\0", "\0", "\0", 0),
    // M1.1: these 11 programs existed but were never registered in a suite.
    // adder_atomic takes (threads, iterations); the smaller count keeps the
    // suite's runtime bounded without weakening the lock it checks.
    ("adder_atomic\0", "4\0", "1000\0", "\0", 0),
    ("ch8_deadlock_mutex1\0", "\0", "\0", "\0", 0),
    ("ch8_deadlock_sem1\0", "\0", "\0", "\0", 0),
    ("ch8_deadlock_sem2\0", "\0", "\0", "\0", 0),
    ("early_exit\0", "\0", "\0", "\0", 0),
    ("early_exit2\0", "\0", "\0", "\0", 0),
    ("eisenberg\0", "\0", "\0", "\0", 0),
    ("peterson\0", "\0", "\0", "\0", 0),
    ("stackful_coroutine\0", "\0", "\0", "\0", 0),
    ("stackless_coroutine\0", "\0", "\0", "\0", 0),
    ("usertests_simple\0", "\0", "\0", "\0", 0),
    // M1.3a: EFAULT/EINVAL/ENOSYS negative cases
    ("syscall_errors\0", "\0", "\0", "\0", 0),
    // M2.1: the re-landed lab programs (docs/reland.md §9)
    ("mmap0\0", "\0", "\0", "\0", 0),
    ("mmap3\0", "\0", "\0", "\0", 0),
    ("unmap\0", "\0", "\0", "\0", 0),
    ("unmap2\0", "\0", "\0", "\0", 0),
    ("brk_test\0", "\0", "\0", "\0", 0),
    ("dup2_test\0", "\0", "\0", "\0", 0),
    ("spawn0\0", "\0", "\0", "\0", 0),
    ("spawn1\0", "\0", "\0", "\0", 0),
    ("ch5_setprio\0", "\0", "\0", "\0", 0),
    ("ch5_stride\0", "\0", "\0", "\0", 0),
    ("trace_test\0", "\0", "\0", "\0", 0),
    // M2.2: the VFS with paths seed suite (docs/vfs.md §12)
    ("path_test\0", "\0", "\0", "\0", 0),
    ("dir_test\0", "\0", "\0", "\0", 0),
    ("link_test\0", "\0", "\0", "\0", 0),
    ("rename_test\0", "\0", "\0", "\0", 0),
    ("lseek_test\0", "\0", "\0", "\0", 0),
    ("stat_test\0", "\0", "\0", "\0", 0),
    ("open_flags_test\0", "\0", "\0", "\0", 0),
    ("cloexec_test\0", "\0", "\0", "\0", 0),
    // M2.3: permissions, mounts and /dev seed suite (docs/perm.md §11)
    ("perm_test\0", "\0", "\0", "\0", 0),
    ("dev_test\0", "\0", "\0", "\0", 0),
    ("mount_test\0", "\0", "\0", "\0", 0),
    // M2.4: the driver and the buffer cache under load (docs/block.md §10)
    ("disk_stress_test\0", "\0", "\0", "\0", 0),
    // M2.5: ELF loader, shebang and envp (docs/elf.md §7)
    ("elf_test\0", "\0", "\0", "\0", 0),
];

static FAIL_TESTS: &[(&str, &str, &str, &str, i32)] = &[
    ("stack_overflow\0", "\0", "\0", "\0", -11),
    ("priv_csr\0", "\0", "\0", "\0", -4),
    ("priv_inst\0", "\0", "\0", "\0", -4),
    ("store_fault\0", "\0", "\0", "\0", -11),
    ("until_timeout\0", "\0", "\0", "\0", -6),
    ("adder\0", "\0", "\0", "\0", -6),
    ("adder_simple_spin\0", "\0", "\0", "\0", -6),
    ("adder_simple_yield\0", "\0", "\0", "\0", -6),
    // M2.1: a bad mmap protection must fault on any access, not write
    ("mmap1\0", "\0", "\0", "\0", -11),
    ("mmap2\0", "\0", "\0", "\0", -11),
];

use user_lib::{exec, fork, waitpid};

fn run_tests(tests: &[(&str, &str, &str, &str, i32)]) -> i32 {
    let mut pass_num = 0;
    let mut arr: [*const u8; 4] = [
        core::ptr::null::<u8>(),
        core::ptr::null::<u8>(),
        core::ptr::null::<u8>(),
        core::ptr::null::<u8>(),
    ];

    for test in tests {
        println!("Usertests: Running {}", test.0);
        arr[0] = test.0.as_ptr();
        if test.1 != "\0" {
            arr[1] = test.1.as_ptr();
            arr[2] = core::ptr::null::<u8>();
            arr[3] = core::ptr::null::<u8>();
            if test.2 != "\0" {
                arr[2] = test.2.as_ptr();
                arr[3] = core::ptr::null::<u8>();
                if test.3 != "\0" {
                    arr[3] = test.3.as_ptr();
                } else {
                    arr[3] = core::ptr::null::<u8>();
                }
            } else {
                arr[2] = core::ptr::null::<u8>();
                arr[3] = core::ptr::null::<u8>();
            }
        } else {
            arr[1] = core::ptr::null::<u8>();
            arr[2] = core::ptr::null::<u8>();
            arr[3] = core::ptr::null::<u8>();
        }

        let pid = fork();
        if pid == 0 {
            exec(test.0, &arr[..]);
            panic!("unreachable!");
        } else {
            let mut exit_code: i32 = Default::default();
            let wait_pid = waitpid(pid as usize, &mut exit_code);
            assert_eq!(pid, wait_pid);
            let passed = exit_code == test.4;
            if passed {
                // summary apps with  exit_code
                pass_num = pass_num + 1;
            }
            // One machine-readable line per test; verify.sh parses these. The
            // leading newline ends any partial output the test left behind.
            print!(
                "\nRESULT {} expected={} actual={} {}\n",
                test.0.trim_end_matches('\0'),
                test.4,
                exit_code,
                if passed { "PASS" } else { "FAIL" }
            );
            println!(
                "\x1b[32mUsertests: Test {} in Process {} exited with code {}\x1b[0m",
                test.0, pid, exit_code
            );
        }
    }
    pass_num
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let succ_num = run_tests(SUCC_TESTS);
    let err_num = run_tests(FAIL_TESTS);
    if succ_num == SUCC_TESTS.len() as i32 && err_num == FAIL_TESTS.len() as i32 {
        println!(
            "{} of sueecssed apps, {} of failed apps run correctly. \nUsertests passed!",
            SUCC_TESTS.len(),
            FAIL_TESTS.len()
        );
        return 0;
    }
    if succ_num != SUCC_TESTS.len() as i32 {
        println!(
            "all successed app_num is  {} , but only  passed {}",
            SUCC_TESTS.len(),
            succ_num
        );
    }
    if err_num != FAIL_TESTS.len() as i32 {
        println!(
            "all failed app_num is  {} , but only  passed {}",
            FAIL_TESTS.len(),
            err_num
        );
    }
    println!(" Usertests failed!");
    return -1;
}
