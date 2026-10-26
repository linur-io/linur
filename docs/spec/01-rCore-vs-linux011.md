# linur vs Linux 0.11 — measured gap analysis

Status: draft v0.2
Baselines: Linux 0.11 = pristine `linux-0.11.tar.gz` from kernel.org, sha256 `660997a6b1f03ee5aadce0ac806ffa653dedf1dea6d82c6731f9c9ddf0092673`, cross-checked on elixir.bootlin.com. linur = the rCore snapshot at HEAD `4766af7` ("ch8-lab: deadlock detection"). Every linur claim below cites a file, a grep, or a measurement taken in this session; every 0.11 claim was read out of that tarball. Numbers quoted from the annotated Chinese tree (Zhao Jiong) are not used — that tree is ~20% larger by line count.

## 0. Method and evidence

* Read-only source recon of `os/`, `user/`, `easy-fs/`, `easy-fs-fuse/` with file:line citations.
* Host measurements performed in this session (see §6): full build, QEMU boots, a staged-image harness at `/tmp/linur-stage` that packs and boots an arbitrary userland without touching the build cache.
* Claims marked **(unverified)** were not confirmed from primary sources.

## 1. Scale

| | Linux 0.11 | linur @4766af7 |
|---|---|---|
| kernel source | 13,839 lines in ~100 files (51 .c = 8,578; 33 .h = 2,741; 8 .s = 1,935; 8 Makefiles = 585) | 3,992 lines in `os/src` |
| userland | **not in the tree** — separate MINIX root images; `lib/` (392 lines) is a kernel-resident mini-libc for `init` | 3,715 lines in `user/src` (a bespoke libc-ish + 55 test programs) |
| filesystem | `fs/` 3,239 lines (VFS + MINIX v1 + buffer cache) | `easy-fs` 1,240 + `easy-fs-fuse` 151 lines |
| total | 13,839 (kernel only) | 9,098 |
| largest subsystems | `fs/` 3,239; `kernel/` 2,106; `chr_drv/` 2,057 | `mm/`, `task/`, `syscall/`, `fs/` |

## 2. Shape statement

**0.11 is complete but shallow**: every subsystem a UNIX needs exists — a three-stage boot chain, working drivers, a tty line discipline, a VFS, a buffer cache with hash queues and a request elevator, COW plus demand paging, signals with real user handlers, sessions and process groups — each in a few hundred lines, and none of it deeper than it needed to be.

**linur is deep but narrow**: it has a modern page-table VM (`SV39`, 3 levels), a thread/PCB split with mutex/semaphore/condvar and Banker's-algorithm deadlock detection (facilities 0.11 never had), and an on-disk FS with indirect blocks — sitting on SBI, with no boot chain, no drivers of its own, no paths, no permissions, no errno, no tty, no accounting, and a habit of panicking where 0.11 returns an error.

So the project is not "add the missing features". It is: **build the missing half of a UNIX, and re-specify the interface layer underneath the half that already exists.**

## 3. Subsystem matrix

| Subsystem | Linux 0.11 (measured) | linur @4766af7 (verified) | Work to L1 |
|---|---|---|---|
| **Boot chain** | `bootsect.s` exactly 512 B → `setup.s` ≤ 4 sectors (real mode, INT 15h memory size, moves image to 0x00000, temp GDT/IDT, CR0.PE) → `head.s` at 0x0000 (A20 check, 287/387 detect, pg_dir + 4 page tables = 16 MiB identity map, jumps to `main`). System ≤ 128 KiB at build time. | S-mode entry from a **vendored binary** `bootloader/rustsbi-qemu.bin` (RustSBI 0.3.1, built on someone else's WSL box); kernel loaded by QEMU `-device loader` at 0x80200000; `entry.asm` sets a stack and jumps to Rust. | Own x86_64 boot chain (multiboot2 or an own loader + long mode setup); for riscv64, build RustSBI in CI or write an M-mode shim. |
| **Arch abstraction** | n/a (i386 only) | one hard-coded board file `boards/qemu.rs` (CLOCK_FREQ 12.5 MHz, MEMORY_END 0x8800_0000, MMIO list) wired by `#[path]`; `cfg(feature)` = 0 hits in the whole repo | HAL seam + per-arch crates; real trait design at port #2 |
| **Physical memory** | `mem_map[]` byte array over 1–16 MiB; `get_free_page` scans **backwards**, zero-fills; `PAGING_PAGES` 3840 | stack allocator (bump + LIFO recycle, no coalescing) over `[ekernel, 0x8800_0000)`; every frame zeroed; kernel heap = `buddy_system_allocator` (Tier-0 violation) in a 2 MiB static array | own allocator — **in C**, per D8 |
| **Virtual memory** | segmentation + paging: GDT/LDT, 64 tasks × 64 MiB window, total 4 GiB; kernel identity-mapped; `copy_page_tables` clears the write bit (**COW**), `un_wp_page`/`do_wp_page`/`write_verify`, `do_no_page` (**demand paging**), `share_page`/`try_to_share` (executable page sharing). **No swap, no mmap, ever.** | `SV39`, 3-level, hard-coded; eager mapping (every frame allocated at map time); fork = full copy, **no COW**; no demand paging, no stack growth (fixed 8 KiB), no shared memory, no `mmap`/`brk` in HEAD (they existed in `7709ba3`, dropped by ch5) | COW fork, demand paging, `brk`/`mmap`, stack growth |
| **Process model** | `NR_TASKS` 64, `task[]` array, `find_empty_process`; TSS hardware task switch; 5 states; `copy_process` + `copy_mem`; sessions/groups/`leader`/`kill_session`; controlling tty; `utime`/`stime`/`cutime`/`cstime` | PCB (address space, fds, signals) + TCB (context, kernel stack) split — threads exist here and **not** in 0.11; fork asserts `thread_count()==1`; FIFO round-robin, 100 Hz tick; zombie + reparent to INITPROC; no sessions/pgrp/tty; no time accounting | rlimit-free but real: sessions, pgrp, tty ownership, accounting |
| **Signals** | 1–22 defined, `_NSIG` 32; bitmask + `blocked`; SIGKILL/SIGSTOP unblockable; `signal`/`sigaction`; delivery on the user-mode return path builds a frame on the user stack, restorer-based (**no `sigreturn` syscall**); default action = `do_exit(1<<(signr-1))` | 5 bits total: SIGINT/SIGILL/SIGABRT/SIGFPE/SIGSEGV, **no handlers**, no mask, no pending set, no SIGKILL/SIGTERM/SIGCHLD/SIGSTOP; `kill(pid, 9)` fails because bit 9 is undefined | handlers + masks + proper numbers; `sigsuspend`-class blocking |
| **Time** | HZ 100, `LATCH` 11931 into PIT ch0; `jiffies`; RTC read once at boot via ports 0x70/0x71 + `kernel_mktime`; `alarm()`, `times()`, `pause()`; **no** `setitimer`/`gettimeofday` | 100 Hz S-timer; `get_time` = monotonic ms since boot from the `time` CSR (**not wall clock**); `sleep(ms)` on a timer heap; no RTC use, no alarm, no CPU accounting | wall clock + alarm + accounting |
| **Syscall ABI** | `int 0x80`, eax = number, **max 3 args** (ebx/ecx/edx), return in eax; 72 entries; out-of-range → **-1**; 13 entries are `-ENOSYS` stubs | same-shape dispatch but a0 = x17, **max 3 args**, return in a0; 28 entries; **no errno** (`-1`/`-2` sentinels); unknown id and unknown `open` flags **panic the kernel** | full Linux-shaped layer: 6 args, errno, per-arch syscall tables, ENOSYS instead of panic |
| **Fault handling** | `verify_area` before user writes; page fault → `do_no_page`/`do_wp_page`; bad pointers reported | any trap taken in S-mode **panics**; a bad user pointer `unwrap()`s inside the copy helper and **panics the kernel** | fault-safe `copy_from_user`/`copy_to_user` returning `EFAULT` — prerequisite for testing anything |
| **VFS** | no operation vectors at all: `sys_read`/`sys_write` switch on inode mode bits to `rw_char`/`block_read`/`file_read`/`read_pipe`; `m_inode`/`file`/`super_block`/`buffer_head`; `NR_OPEN` 20, `NR_INODE` 32, `NR_FILE` 64, `NR_SUPER` 8, `NR_HASH` 307 | `trait File {readable, writable, read, write}` + `OSInode`/`Stdin`/`Stdout`/`Pipe`; per-open-file offset; one global root inode; no dentry/inode/superblock objects, no mount table | real VFS objects + path walker + mount table |
| **Paths & names** | component-by-component walk straight out of user memory; **14-char names**; no MAX_PATH check; `.`/`..` as real entries; trailing slash means directory | single name looked up in the root directory; **no `/`, no `.`, no `..`, no cwd, no mounts**; 28-byte names (easy-fs) | directories, `.`/`..`, cwd, `chdir`/`getcwd`, long names |
| **On-disk FS** | MINIX v1 only: 1024-byte blocks, 32-byte inodes, 16-byte dir entries, 7 direct + 1 single + 1 double indirect zones, superblock magic 0x137F; root device baked into the boot sector | easy-fs: 512-byte blocks, 28-byte names + u32 inode number, 28 direct + indirect1 + indirect2, no permissions/uid/gid/timestamps/nlink | decide format (round 2/3): keep easy-fs lineage vs adopt a MINIX-v1-compatible layout |
| **Buffer cache** | descriptors at `&end`, data growing down from `buffer_memory_end` (skipping the 0xA0000–0x100000 hole); ~thousands of 1 KiB buffers; **hash queue (307) + free list**; `bread`/`breada` (read-ahead)/`bread_page`; `ll_rw_block` → `make_request` → elevator (`NR_REQUEST` 32, reads preferred, 2-sector requests) | easy-fs has a **16-block write-back cache**, FIFO eviction, `panic!` when every block is pinned, and it flushes the whole cache on every write | real cache: hash queues, delayed write, per-inode/page caches |
| **Permissions** | `permission(inode, mask)` with owner/group/other and `suser()`; `chmod`/`chown`/`access`/`umask`; uid/gid/euid/egid/suid/sgid fields | **none** — no uid/gid/mode on disk, no permission checks | modes + ownership + umask + chmod/chown |
| **Pipes** | pipe-inode, one page (4095-byte capacity), readers/writers sleep on `i_wait`, **SIGPIPE** when no readers | 32-byte ring, blocking, EOF when all writers closed, **no read-end tracking → no EPIPE/SIGPIPE**; writer to an abandoned pipe blocks forever | POSIX pipe semantics |
| **Console / tty** | VGA/CGA/MDA text console 80x25 with a full ANSI parser and beeper; `tty_table[3]` = console + COM1 + COM2; `copy_to_cooked` with ICANON/ECHO/ISIG; ^C→SIGINT, ^\\→SIGQUIT, **^Z does nothing** (`TSTPMASK` defined, never wired); 1024-byte queues | SBI `putchar`/`getchar` only; stdin `read` **asserts len == 1** (a `read(0, buf, n>1)` panics); no tty, no line discipline, no echo, no canonical mode, no job control | tty layer + line discipline; console device |
| **Keyboard** | `keyboard.S`: IRQ1, scancode set 1 maps, shift/caps/ctrl/alt, F1–F12, LEDs, ctrl-alt-del; default layout KBD_FINNISH | none | PS/2 driver |
| **Interrupts** | 8259 master+slave, per-device IRQ unmasking, EOI; **IRQ-driven** block I/O with a request queue | none: timer via SBI, block I/O **polled**, no PLIC, no IRQ-driven I/O at all; S-mode traps panic if unexpected | PIC/APIC + IRQ framework + DMA |
| **Block devices** | floppy (FDC + DMA) and ST-506/ATA (0x1f0–0x1f7, CHS, MAX_HD 2), optional ramdisk (off by default); partition tables read at boot | one driver: virtio-MMIO at 0x1000_1000 via the `virtio-drivers` crate, polled, DMA frames **leaked permanently** | own block driver + request layer |
| **Char devices** | `/dev/null`, real `/dev/port` (inb/outb over 64 KiB), `/dev/mem`/`/dev/kmem`/`/dev/ram` all return `-EIO`; major-number dispatch tables | no device nodes, no `/dev`, no `ioctl`, no `mknod` | `/dev` + major/minor dispatch |
| **Config** | `Makefile` + `config.h` (`RAMDISK` commented out; keyboard layout KBD_FINNISH vs KBD_US/GR/FR) | hard-coded `const`s in `config.rs`; no features, no Kconfig, no build-time config | `config/<board>.toml` → generated consts (D6) |
| **Tests / CI** | none in tree (lab courses bolt on `testlab2.c` etc.) | no CI, no `test` target, one host test (`easy-fs-fuse::efs_test`, passing); `make run TEST=1` is interactive yet **does** return QEMU exit 0 / 255 (measured); 16 of 55 apps never exercised; `usertests` passes only by accident (a missing app counts as an expected failure) | `make verify` gate + real matrix |
| **Userland / libc** | no userland shipped; `lib/` mini-libc; programs are a.out ZMAGIC with `N_TXTOFF == 1024`, static, no shared libs; `init` execs `/etc/rc` then `/bin/sh` | bespoke `user_lib` (no errno, no environ, no stdio buffering); 55 test programs including a shell with `<`, `>`, `|` and no builtins; apps are RISC-V ELFs packed into `fs.img` | own libc + shell + coreutils; ELF (already ELF) |
| **ELF / exec** | a.out **ZMAGIC only**, `#!` scripts supported, argv/envp on MAX_ARG_PAGES | ELF `PT_LOAD` with U/R/W/X, no `envp`, no PIE/ASLR, no `CLOEXEC` | full `execve` with envp, shebang, `CLOEXEC` |

## 4. Syscall coverage

* 0.11: **72** entries (0–71); **13** are `-ENOSYS` stubs; **59** are implemented.
* linur: **50** entries — 33 with Linux/riscv64 numbers (`getcwd` 17, `dup` 24, `mknod` 33, `mkdir` 34, `unlink` 35, `link` 37, `rename` 38, `access` 48, `chdir` 49, `chmod` 53, `chown` 54, `open` 56, `close` 57, `pipe` 59, `getdents64` 61, `lseek` 62, `read` 63, `write` 64, `stat` 79, `fstat` 80, `exit` 93, `sleep` 101, `yield` 124, `kill` 129, `umask` 166, `get_time` 169, `getpid` 172, `brk` 214, `munmap` 215, `fork` 220, `exec` 221, `mmap` 222, `waitpid` 260), one lab syscall (469), 12 teaching-only numbers (1000–1032: threads, mutex, semaphore, condvar), and four documented extensions (1040–1043: `spawn`, `trace`, `set_priority`, `dup2`). The 1040–1043 family follows the 1000+ extension convention; `dup2` is there because riscv64 Linux keeps only `dup3`. `creat` is not a table entry: it is the libc wrapper over `open`, as on Linux.
* Overlap with 0.11's implemented set: **29** names (`read`, `write`, `open`, `close`, `dup`, `pipe`, `exit`, `fork`, `execve`, `waitpid`, `getpid`, `kill`, `time`, `brk`, `dup2`, plus the M2.2 additions `creat`, `link`, `unlink`, `chdir`, `stat`, `lseek`, `fstat`, `mkdir`, `rmdir`, and the M2.3 additions `mknod`, `chmod`, `chown`, `access`, `umask`) — and **nine of those diverge** in semantics (monotonic-ms `time`, 5-bit `kill`, non-blocking `waitpid`, `open` returning `ENOTDIR` for `O_CREAT|O_DIRECTORY` on a missing path where Linux returns `EINVAL`, `brk` returning the new break where 0.11 returns 0, `dup2` refusing a `newfd` past the current table where 0.11 accepts any descriptor below `NR_OPEN`, `stat` filling the Linux riscv64 128-byte layout where 0.11 fills the i386-era one, `creat` existing only as the libc wrapper over `open` where 0.11 has a syscall, and `chown` taking `u32::MAX` as "leave that half unchanged" where 0.11 assigns the argument literally).
* Missing but implemented by 0.11 (≈30): `setup`, `mount`, `umount`, `setuid`/`getuid`/`geteuid`/`setreuid`, `setgid`/`getgid`/`getegid`/`setregid`, `stime`, `alarm`, `pause`, `utime`, `nice`, `sync`, `times`, `signal`, `sigaction`, `sgetmask`/`ssetmask`, `ioctl`, `fcntl`, `setpgid`, `uname`, `chroot`, `getppid`, `getpgrp`, `setsid`.
* Present in linur and absent from 0.11: `getcwd`, `rename` (0.11 declares number 38 but stubs it with `-ENOSYS`), `getdents64`, the 12 thread/sync syscalls, `sleep` (ms), `yield`, `enable_deadlock_detect`, `spawn`, `trace`, `set_priority`, plus a planned documented extension in the same 1000+ family: a process-info call for `ps` (v1.0 has no `/proc`, and 0.11 offers no interface `ps` could use).

**ABI consequence:** a Linux-shaped interface (D1/L4) cannot reuse one table for two architectures — x86_64 and riscv64 Linux number their syscalls differently (`open` is 2 on x86_64, 56 on riscv64; `fork` is 57 vs 220). The dispatch layer must be per-arch with a shared semantic core. (Recorded as an open question.)

## 5. Deliberate divergences (fidelity deltas)

These are choices, not gaps — each one must stay listed so the claim stays honest.

| # | Divergence | Why |
|---|---|---|
| F1 | Linux-shaped ABI (6 args, errno, per-arch tables) instead of 0.11's 3-arg/-1 ABI | test oracles: musl/busybox/LTP-shaped suites become reachable |
| F2 | ELF instead of a.out ZMAGIC | toolchain reality in 2026 |
| F3 | modern paging (SV39 / 4-level x86_64) instead of 0.11's segment+paging hybrid | 0.11's 64 MiB windows and LDTs teach nothing extra and cost a lot |
| F4 | no 16 MiB RAM ceiling, no 64-task limit | artificial limits are not a feature |
| F5 | threads + sync primitives + deadlock detection **kept** from rCore | 0.11 has none; it is the one place linur starts ahead |
| F6 | virtio-MMIO **for now** instead of ATA/floppy; own driver later | floppy DMA in QEMU is a museum piece; an ATA driver is planned as a real-hardware path |
| F7 | HZ 100 kept; wall-clock RTC planned | 0.11's clock drifts because it reads the RTC once; we keep the tick but fix the clock |
| F8 | config generated from TOML instead of `config.h` | D6 |
| F9 | no original 0.11 userland or MINIX images | L3 is a later optional stage |
| F10 | GPLv3 inherited from rCore | legal constraint of the snapshot |
| F11 | long file names instead of 0.11's 14-character names | the easy-fs lineage already stores 28-byte names; 14 characters is an artificial limit |
| F12 | own on-disk format continuing the easy-fs lineage instead of MINIX v1 | L3 (original 0.11 images) is a later optional stage; easy-fs already exists |
| F13 | wall clock that does not drift: x86_64 gets a self-built CMOS RTC driver; riscv64 is an architecture-scoped delta — QEMU riscv `virt` has no RTC device (verified on QEMU 8.2.2), so wall clock = `stime`-set boot epoch + jiffies | 0.11 reads the RTC once and drifts; the second port must not drag in an environment-dependent device model |
| F14 | scheduler-algorithm freedom: a fair preemptive scheduler; the stride re-landing landed in M2.1 and is the concrete algorithm | user-visible fairness is the point, not replicating 0.11's counter + refill scheme |

## 6. Measured defects in the baseline (Stage 0 input)

1. **`make run TEST=1` corrupts the cargo build cache.** `user/Makefile:18` does `cp usertests initproc`; cargo hardlinks `target/release/<bin>` to `target/release/deps/<bin>-<hash>`, so the `cp` writes *through* the hardlink into the cached artifact. Every later build then "restores" the corrupted binary. Observed: `initproc` became byte-identical to `hello_world` with **zero** `user_shell` strings; repaired by `touch user/src/bin/initproc.rs` + rebuild (`user_shell` strings 0 → 1). A repeated `TEST=1` run therefore risks testing the wrong program.
2. **`make build` needs the network.** It depends on the phony `env` target, which runs `cargo install cargo-binutils` on every build — although `rust-objcopy` is already installed.
3. **`usertests` passes by accident.** `race_adder_loop` is referenced but absent; the missing app is counted as one of the 9 expected failures, so the tally still matches. 16 of 55 programs are never exercised, including all three ch8-lab deadlock tests, `peterson`, `eisenberg`, both coroutine tests, `early_exit*`, `count_lines`.
4. **`docs/` is untracked** (0 files in `git ls-files docs`), and `/AGENTS.md` is hidden by `.gitignore`, so the repo's own agent guide is not committed. `README.md` is deleted in the working tree.
5. **Kernel panics on user error**: bad user pointer (copy helper `unwrap()`), unknown `open` flag, unknown syscall id, any S-mode trap. No test can assert `EFAULT`/`ENOSYS` while this is true.
6. **`read(0, buf, n>1)` panics** (`assert_eq!(user_buf.len(), 1)` in `fs/stdio.rs`).
7. **Pipe writers to an abandoned pipe block forever** (no read-end tracking, no EPIPE/SIGPIPE).
8. **`fork`/`exec` assert a single thread**, so a multi-threaded process cannot fork.
9. **Tier-0 dependency violations**: `xmas-elf` (ELF parsing), `buddy_system_allocator` (kernel heap), `virtio-drivers` pinned to a git rev, `sbi-rt`, plus the vendored RustSBI binary.
10. **The lab features are gone from HEAD**: `mmap`/`munmap`/`sbrk` (`7709ba3`), `sys_trace` (`851e377`), `spawn` (`c505a09`), stride + `setprio` (`2a242b5`), `link`/`unlink`/`fstat` (`c82eb2d`). They are recoverable as reference diffs (~1,500 lines including their test programs, which are worth keeping as tests).

## 7. Where linur is ahead of 0.11

Page-table VM with per-region permissions; PCB/TCB separation with real threads; mutex/semaphore/condvar with wait queues and Banker's-algorithm deadlock detection; an on-disk FS with a block cache and multi-level indexing; a user-space shell; a Rust toolchain and a workspace; a test app corpus (55 programs) that 0.11 never had.

## 8. Open questions

Carried into round 2/3: time realism and stage count; repository name and history policy; the C boundary; stage ordering; whether re-landed lab features adopt the Linux ABI immediately; per-arch syscall tables; console and userland target for the "use it myself" criterion; on-disk FS format; whether riscv64 keeps 3-arg-style dispatch or moves to the Linux 6-arg convention first.

## Appendix A — Recovered lab commits

Five reverted lab implementations live only in the frozen public repo `Fux-i/rCore` (reachable from its `main` branch), never in linur's history. Roadmap step M2.1 needs them as reference diffs:

| lab | commit | contents |
|---|---|---|
| ch4-lab | `7709ba3` | `mmap` / `munmap` / `sbrk` |
| ch3-lab | `851e377` | `sys_trace` |
| ch5-lab1 | `c505a09` | `spawn` |
| ch5-lab2 | `2a242b5` | stride scheduling + `setprio` |
| ch6-lab | `c82eb2d` | `link` / `unlink` / `stat` |

M2.1 re-lands all five: the semantics are ported to the M1 ABI, not cherry-picked (`docs/reland.md`).
