# linur — Linux 0.11 parity checklist (the v1.0 contract)

Status: draft v0.2. This file is written **before** M1 starts and updated at every milestone gate.

## How to use it

Every row must end as exactly one of:

| status | meaning |
|---|---|
| `TODO` | not started; a milestone is named |
| `WIP M# → DONE M#` | partially present today and being brought to spec; names the milestone that flips the row to DONE |
| `DONE` | implemented, tested by the gate, and evidenced |
| `DELTA F#` | deliberately different from 0.11; the fidelity-delta id in `spec/01` §5 gives the reason |
| `DEFER` | out of scope for v1.0, with a stated reason |

**v1.0 is reached when no row is TODO or WIP on either architecture, and every DELTA and DEFER row carries a reason.** The owner reviews this file at each milestone gate.

## 1. Boot and machine

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| boot sector stage | `bootsect.s`, exactly 512 B, loads 4 sectors | own x86_64 loader, no 512-byte constraint | TODO M5.4 |
| machine setup | `setup.s` real mode: video params, memory size, moves image, GDT/IDT, CR0.PE | long-mode entry, memory detection, GDT/IDT/TSS | TODO M5.4 (the M1.5 smoke path already enters long mode from its own loader and writes serial; memory detection, IDT/TSS and the rest are M5.4 — `docs/x86_64-smoke.md`, `verify/x86-serial.log`) |
| memory ceiling | 16 MiB physical cap | no artificial cap | DELTA F4 |
| task ceiling | `NR_TASKS` 64 | no artificial cap | DELTA F4 |
| second architecture | none (i386 only) | x86_64 flagship + riscv64 port, both CI-green | TODO M5.4 (from M1.5 CI runs the x86_64 smoke path — own loader → long mode → serial → clean exit — plus the compile-only `cargo check --target x86_64-unknown-none`; the full suite on both architectures is M5.4 — `verify/report.json`) |
| boot path ownership | self-contained assembly, self-hosting | no vendored third-party binary in the boot path | TODO M5.5 (the x86_64 chain has been ours since M1.5; riscv64 still boots under the vendored RustSBI) |

## 2. Memory

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| frame allocator | `mem_map[]` byte array, backwards scan | own allocator, written in C (D8) | TODO M2.6 |
| address translation | segmentation + paging, LDT per task, 64 MiB window | page-table VM per arch | DELTA F3 |
| copy-on-write | `copy_page_tables` clears the write bit; `un_wp_page` | COW `fork` | TODO M4.1 |
| demand paging | `do_no_page`, page from the executable | demand paging + page cache | TODO M4.2 |
| shared executable pages | `share_page` / `try_to_share` | shared pages | TODO M4.3 |
| heap growth | `sys_brk` moves an end pointer, pages fault in | `brk` with the same user-visible behaviour | WIP M2.1 → DONE M4.3 (eager `brk` in M2.1; demand paging in M4.3) |
| `mmap` | **absent** | present (needed by the L4-shaped ABI and by modern userland) | DELTA F1 (present since M2.1: anonymous, fixed address, eager; M4.3 adds fd/offset and shared pages) |
| out-of-memory | `oom()` kills a task or panics | explicit policy with accounting | TODO M4.4 |

## 3. Process

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| process table | `task[]` array, index 0 is the idle task | same user-visible behaviour | TODO M3.5 (completes with sessions/pgrp) |
| states | RUNNING / INTERRUPTIBLE / UNINTERRUPTIBLE / ZOMBIE / STOPPED | same five states | WIP M1.3 → DONE M1.3 |
| `fork` | COW, child returns 0 | same, with COW | TODO M4.1 |
| `execve` | a.out ZMAGIC only, shebang supported, argv/envp | ELF, shebang, `envp` | DELTA F2 |
| `exit` / `waitpid` | reparent to init, options WNOHANG/WUNTRACED, cutime/cstime | same semantics, blocking by default | WIP M1.3 → DONE M6.1 |
| sessions and groups | `pgrp`, `session`, `leader`, `setsid`, controlling tty | same | TODO M3.5 |
| accounting | utime / stime / cutime / cstime | same | TODO M3.5 |
| scheduler | counter + priority refill, preempt only in user mode | fair preemptive scheduler; exact algorithm may differ (F14) | DONE (stride, DELTA F14) |
| threads | **absent** | present via the kept rCore syscalls 1000–1032 until `clone`/`futex` land | DELTA F5 |

## 4. Signals

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| numbers | 1–22, `_NSIG` 32 | same numbering | TODO M3.2 |
| delivery | user-stack frame with restorer, on the user-mode return path | same mechanism | TODO M3.2 |
| mask | `blocked` word; SIGKILL and SIGSTOP unblockable | same | TODO M3.2 |
| API | `signal`, `sigaction`, `sgetmask`, `ssetmask` | same names and semantics | TODO M3.2 |
| default actions | SIGCHLD ignored, otherwise terminate with the signal bit as exit code | same | TODO M3.2 |
| job control | **absent** (^Z is defined but never wired) | absent for v1.0 | DEFER: not in 0.11 either |

## 5. Filesystem

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| VFS layer | fixed switch statements on inode mode bits | real inode/dentry/superblock objects | DONE (M2.2: superblock, dentry and inode objects) |
| paths | component walk, no MAX_PATH check | walker with dot, dotdot, cwd | DONE (M2.2: `.`/`..` through real entries; 27-byte components, 4096-byte paths) |
| name length | 14 characters | long names | DELTA F11 |
| on-disk format | MINIX v1, 1 KiB blocks, 32-byte inodes, 7+1+1 zones | own format on the easy-fs lineage | DELTA F12 (v2: `nlink`, 27 direct entries, real `.`/`..` entries) |
| buffer cache | hash queue + free list, read-ahead, elevator | hash queues, delayed write, `sync` | TODO M2.4 |
| mounts | table of 8, MINIX only, no fs-type argument | mount table, at least one filesystem | TODO M2.3 |
| permissions | owner/group/other, `suser()`, `chmod`, `chown`, `umask` | same | TODO M2.3 |
| fd table | `NR_OPEN` 20, global file table of 64 | no artificial limit | DELTA F4 (per-descriptor `CLOEXEC` since M2.2) |
| pipes | pipe inode, one page, `SIGPIPE` with no readers | same, with `SIGPIPE` | TODO M6.2 |
| device nodes | `/dev/null`, `/dev/port` real; mem devices return EIO | same, honest behaviour | TODO M6.2 |
| file syscalls | stat, fstat, lseek, dup, dup2, link, unlink, mkdir, rmdir, mknod, chmod, chown, utime, access, umask, sync, truncate | all present | WIP M2.2 → DONE M6.1 (M2.2 adds `stat`, `fstat`, `lseek`, `link`, `unlink`, `mkdir`, `rmdir`; remains `mknod` M2.3, mode/uid/gid checks and `chmod`/`chown`/`access`/`umask` M2.3, `utime` M3.5, `sync` M6.1) |
| `ioctl` / `fcntl` | tty ioctls and terminal line settings; a partial `fcntl` | same subset | TODO M6.1 |
| symlinks, `/proc` | absent | absent for v1.0 | DEFER |

## 6. Devices and drivers

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| console | VGA/CGA/MDA text, ANSI parser, 80x25, scrolling, beeper | VGA text console with ANSI | TODO M3.4 |
| keyboard | PS/2 scancode set 1, modifiers, F-keys, ctrl-alt-del | same core behaviour | TODO M3.4 |
| serial | 8250, fixed 2400 bps, two ports | own UART driver, usable speed | TODO M3.1 |
| timer | 8253/8254, HZ 100, LATCH 11931 | same tick rate; own PIT/HPET driver | TODO M5.4 |
| interrupt controller | 8259 master + slave, explicit EOI | own PIC or APIC handling | TODO M5.4 |
| block device | floppy + ST-506/ATA, CHS, request elevator | own driver with a request queue | TODO M2.4 |
| floppy | full FDC + DMA driver | absent | DEFER: museum hardware, no user value |
| ramdisk | optional, off by default | absent for v1.0 | DEFER |
| device layer | major/minor dispatch tables | same shape | TODO M3.1 |

## 7. Time

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| tick | HZ 100, `jiffies` | HZ 100, `jiffies` | WIP M1.3 → DONE M1.3 |
| wall clock (x86_64) | RTC read once at boot, then drifts | own CMOS RTC driver; the clock does not drift | DELTA F13 |
| wall clock (riscv64) | RTC read once at boot, then drifts | no RTC hardware on QEMU `virt`: `stime`-set boot epoch + jiffies | DELTA F13 |
| `time` / `stime` / `times` | seconds since epoch; jiffies; four counters | same | TODO M3.5 (`times` at M6.1) |
| `alarm` / `pause` | single alarm, interruptible pause | same | TODO M3.5 |
| `setitimer` / `gettimeofday` | absent | absent for v1.0 | DEFER |

## 8. Syscall coverage

0.11 defines **72 entries, numbered 0–71**, of which **13 are `-ENOSYS` stubs** (17, 26, 31, 32, 35, 38, 44, 51, 52, 53, 56, 58, 62). linur returns `ENOSYS` for unimplemented ids, which matches the stubs; the 59 implemented calls are tracked below. Since M1.3b the ABI is Linux-shaped: up to six arguments, `-errno` on failure (the table is in `docs/errors.md` §6), and a per-architecture number→handler table.

| # | syscall | linur status |
|---|---|---|
| 0 | `setup` | DEFER: boot-time artifact of 0.11's boot sequence, not a user-visible call |
| 1 | `exit` | DONE |
| 2 | `fork` | WIP M1.3 → DONE M4.1 (COW) |
| 3 | `read` | DONE |
| 4 | `write` | DONE |
| 5 | `open` | DONE (M2.2: Linux flag values, errno, per-descriptor `CLOEXEC`; `mode` accepted but unused until M2.3) |
| 6 | `close` | DONE |
| 7 | `waitpid` | WIP M1.3 → DONE M6.1 (blocking at M1.3; options at M6.1) |
| 8 | `creat` | DONE (M2.2: libc wrapper over `open` with `O_CREAT`, `O_WRONLY` and `O_TRUNC`; no separate number, as on Linux) |
| 9 | `link` | DONE (M2.2: hard link with on-disk `nlink`) |
| 10 | `unlink` | DONE (M2.2; `rmdir` is `AT_REMOVEDIR` 0x200) |
| 11 | `execve` | WIP M1.3 → DONE M2.5 (envp and shebang; errno done at M1.3) |
| 12 | `chdir` | DONE (M2.2: per-process cwd) |
| 13 | `time` | WIP M1.3 → DONE M3.5: today it returns monotonic milliseconds; must return wall-clock seconds |
| 14 | `mknod` | TODO M2.3 |
| 15 | `chmod` | TODO M2.3 |
| 16 | `chown` | TODO M2.3 |
| 18 | `stat` | DONE (M2.2: Linux riscv64 128-byte `struct stat`) |
| 19 | `lseek` | DONE (M2.2: `SEEK_SET`/`SEEK_CUR`/`SEEK_END`) |
| 20 | `getpid` | DONE |
| 21 | `mount` | TODO M6.1 |
| 22 | `umount` | TODO M6.1 |
| 23 | `setuid` | TODO M6.1 |
| 24 | `getuid` | TODO M6.1 |
| 25 | `stime` | TODO M3.5 |
| 27 | `alarm` | TODO M3.5 |
| 28 | `fstat` | DONE (M2.2) |
| 29 | `pause` | TODO M3.5 |
| 30 | `utime` | TODO M3.5 |
| 33 | `access` | TODO M2.3 |
| 34 | `nice` | TODO M3.5 |
| 36 | `sync` | TODO M6.1 (cache flush exists at M2.4) |
| 37 | `kill` | WIP M1.3 → DONE M3.5: today a five-bit mask; must take real signal numbers (M3.2) and support groups (M3.5) |
| 39 | `mkdir` | DONE (M2.2) |
| 40 | `rmdir` | DONE (M2.2: `unlink` with `AT_REMOVEDIR` 0x200) |
| 41 | `dup` | DONE |
| 42 | `pipe` | DONE |
| 43 | `times` | TODO M3.5 |
| 45 | `brk` | DONE (M2.1, eager) |
| 46 | `setgid` | TODO M6.1 |
| 47 | `getgid` | TODO M6.1 |
| 48 | `signal` | TODO M3.2 |
| 49 | `geteuid` | TODO M6.1 |
| 50 | `getegid` | TODO M6.1 |
| 54 | `ioctl` | TODO M6.1 |
| 55 | `fcntl` | TODO M6.1 |
| 57 | `setpgid` | TODO M3.5 |
| 59 | `uname` | TODO M6.1 |
| 60 | `umask` | TODO M2.3 |
| 61 | `chroot` | TODO M6.1 |
| 63 | `dup2` | DONE (extension 1043) |
| 64 | `getppid` | TODO M3.5 |
| 65 | `getpgrp` | TODO M3.5 |
| 66 | `setsid` | TODO M3.5 |
| 67 | `sigaction` | TODO M3.2 |
| 68 | `sgetmask` | TODO M3.2 |
| 69 | `ssetmask` | TODO M3.2 |
| 70 | `setreuid` | TODO M6.1 |
| 71 | `setregid` | TODO M6.1 |

Calls linur has that 0.11 does not: `getcwd` 17, `rename` 38 (0.11's number 38 is an `-ENOSYS` stub) and `getdents64` 61; the 12 thread and synchronisation calls (1000–1032), the lab call 469 and the documented extensions 1040–1043 (`spawn`, `trace`, `set_priority`, `dup2` — `dup2` only because riscv64 Linux keeps `dup3`). The 1040–1043 family follows the 1000+ convention; all are recorded in `spec/01` §4 and are deliberate extensions, not parity gaps.

## 9. Userland and toolchain

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| libc | a small static libc outside the kernel tree | our own libc in `user/` | WIP M3.3 → DONE M3.3 |
| shell | `/bin/sh` (rc) exec'd by `init` | our own shell | TODO M3.3 |
| coreutils | distribution-dependent (ls, cat, cp, rm, mkdir, mount, ps…) | a small set: ls, cat, cd, mkdir, rm, ln, ps, echo | TODO M3.3 |
| executable format | a.out ZMAGIC, static only | ELF, static only | DELTA F2 |
| shared libraries | absent | absent for v1.0 | DEFER |
| init | `/etc/rc` then `/bin/sh` | same shape | TODO M3.3 |

## 10. Configuration and tests

| item | Linux 0.11 | linur target | status |
|---|---|---|---|
| configuration | Makefile plus `config.h` | `config/<board>.toml` → generated consts | DELTA F8, TODO M5.1 |
| test suite | none in tree | `make verify` + host tests + userland suite | DONE (M2.1 adds the 13 lab regression tests; M2.2 adds the 8 VFS programs — 72 in total, `verify/report.txt`) |
| CI | none | every milestone gate, both architectures | WIP M1.4 → DONE M5.4 (per-architecture capability matrix from M1.5: riscv64 full suite, x86_64 smoke path (`docs/x86_64-smoke.md`), x86_64 compile-only check — `verify/report.json`; the x86_64 full suite is M5.4) |
| parity evidence | — | this file, updated at each gate | WIP → DONE M6.4 |

## 11. Deltas registry

`spec/01` §5 carries the narrative and the reasons (F1–F14). The additional deltas that once appeared only in this checklist are now reserved there: F11 long file names instead of 14 characters, F12 the own on-disk format on the easy-fs lineage, F13 the non-drifting wall clock (x86_64 self-built CMOS RTC; riscv64 architecture-scoped), F14 the unspecified-but-fair scheduler. The absence of artificial task/file-descriptor caps stays covered by F4.

## 12. Sign-off

A milestone gate updates the affected rows with a status and evidence (test name, artifact, or commit). v1.0 is tagged only when every row is DONE, DELTA or DEFER with a reason, on both architectures.
