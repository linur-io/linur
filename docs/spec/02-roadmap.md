# linur — Development Roadmap

Status: draft v0.3 (revised in round 4; v0.2 was the round-3 restructure)
Read with `spec/00-requirements.md` (decisions) and `spec/01-rCore-vs-linux011.md` (measured gap). The v1.0 acceptance contract is `spec/03-parity-checklist.md`.

## 1. Planning rules

1. **Milestones carry tags and blog posts; steps do not.** A milestone is tagged and written up; the day-sized steps inside it are ordinary commits.
2. **Day-sized steps.** Anything that cannot honestly fit in a few days is split before work starts.
3. **No tag without a green run.** A milestone ends with `make verify` passing on a clean clone; the tag annotation records the commit and the run.
4. **Spec before code.** Each step's interface is written down before implementation; a module without a written contract cannot later be owned by a classmate.
5. **The checklist is the contract.** `spec/03` exists before M1 starts and is updated by every milestone; v1.0 is reached when it has no unexplained gaps.
6. **Dependencies move down, never up.** A step that replaces a Tier-2 crate deletes it from `Cargo.toml` and `DEPENDENCIES.md` in the same PR.
7. **linur's own tags start at v0.1.0** and follow the milestones; the rCore chapter tags stay in `Fux-i/rCore`. linur has fresh history; the recovered lab commits needed by M2.1 are listed in `spec/01` Appendix A (the fresh-history process record `spec/99-lineage.md` is private and untracked).
8. **Architecture-dependent code lives only inside cfg-gated arch modules.** Enforced as a PR review item from M2 onward.

## 2. Size legend

| size | meaning |
|---|---|
| S | ≈ 1–3 days |
| M | ≈ 4–7 days |
| L | ≥ 2 weeks; must be split into S/M steps before work starts |

## 3. Milestones

### M1 基座 / Foundation (M) → `v0.1.0`

| step | deliverable | size |
|---|---|---|
| M1.1 Hygiene | build without network; staged image packer so `TEST=1` stops corrupting the cargo cache; `usertests` expectation fixed and the 16 unregistered programs enrolled; `.gitignore` enforces D10 (specs 01/02/03 and `AGENTS.md` tracked; the requirements document, the lineage record and the blog posts not tracked); `NOTICE` + SPDX; README rewritten (name, goal, rCore lineage, roadmap link); `deny.toml` created with the D7 three-tier rules and `DEPENDENCIES.md` with one line per crate (purpose, tier, owner, removal issue) | S |
| M1.2 Debug and observability | panic with file:line and a stack trace; boot log that survives a crash; run-one-failing-test isolation in the harness; documented gdb and QEMU-monitor recipes; retire `log` (the own logging facilities built here replace it) | S |
| M1.3a Fault-safe copies and error returns | `copy_from_user` / `copy_to_user`; `ENOSYS` / `EINVAL` / `EFAULT`; every kernel-panic-on-user-error path eliminated; negative tests | S |
| M1.3b Linux ABI and errno | errno conversion, six arguments, per-arch syscall tables (riscv64 first); `user_lib` and all 55 user programs migrated, with full regression | M |
| M1.4 Minimal `make verify` gate | build + boot + exit-code assertion + per-test reporting (riscv64: SiFive test device, code shifted into the high half word-ORed with 0x3333; x86_64: `isa-debug-exit`, verified present in QEMU 8.2.2) + serial log and report artifacts; the CI workflow itself (`git submodule update --init --recursive`, caching, serial-log and report artifacts); a negative test that must fail | S |
| M1.5 x86_64 smoke bring-up | own loader → long mode → serial output → clean exit in CI; deliberately **no** user mode, no devices, no real interrupt handling; a first `hal` seam with two implementations covering console and exit only; the x86_64 compile-only CI check (`cargo check --target x86_64-unknown-none`, no boot required) starts here and is kept until M5.4 lands the full port | M |

**Gate.** Per-architecture capability matrix — every milestone gate states its matrix, and the matrix grows until both architectures run the full suite from M5 on. M1: riscv64 runs `make verify` green on a clean clone with the full suite; x86_64 runs the smoke path (loader → long mode → serial → clean exit) plus the compile-only check. The negative test fails as required; ABI negative tests prove `EFAULT` / `ENOSYS` behaviour; documents committed.
**Not in M1.** Feature work. This milestone exists to make everything after it testable.

### M2 存储与自主模块 / Storage and owned modules (M) → `v0.2.0`

| step | deliverable | size |
|---|---|---|
| M2.1 Re-land the lost functionality | `mmap` / `munmap` / `brk`, `dup2`, `sys_trace`, `spawn`, stride and `setprio` — ported to the M1 ABI; the stride re-landing is the final scheduler (fidelity delta F14); the lab programs become the seed regression suite | M |
| M2.2 VFS with paths | real inode/dentry/superblock objects; path walker; dot and dotdot; cwd, `chdir` / `getcwd`; `mkdir` / `rmdir` / `unlink` / `link` / `rename` / `readdir`; Linux-exact `open` flags; fd semantics including `CLOEXEC`; `stat` / `fstat`, `lseek` and `link` / `unlink` / `nlink` moved here from M2.1 — implemented once, correctly, on the real VFS | M |
| M2.3 Permissions and mounts | mode / uid / gid / umask, permission checks, `chmod` / `chown`; mount table with one filesystem; `/dev` skeleton and device nodes | S |
| M2.4 Own block driver and buffer cache | own virtio-blk (retire `virtio-drivers`); request queue; buffer cache with hash queues, delayed write and `sync`; DMA frames no longer leaked; retire `spin` (own spinlock alongside the request queue) | M |
| M2.5 Own ELF loader | retire `xmas-elf`; shebang scripts; `envp`; malformed image → `ENOEXEC` | S |
| M2.6 C physical allocator | new repo `linur-io/alloc` containing **only** the physical page-frame allocator: freestanding C, tiny C ABI, Rust-provided memory primitives, hand-written `build.rs` plus `clang`, assert-based host tests, mounted as a submodule; retires the stack allocator; in the same step the kernel heap becomes a self-built Rust free-list allocator (≈100 lines, not in the C repo), retiring `buddy_system_allocator` | M |
| M2.7 Filesystem module extraction | `easy-fs` moves to `linur-io/fs`, mounted at `fs/` as a submodule pinned to a tag; host tests move to a std adapter crate; the pin-bump workflow is exercised once | S |

**Gate.** Lab suite + filesystem stress + host tests green; the Tier-2 crates assigned to M2 gone (`virtio-drivers`, `xmas-elf`, `buddy_system_allocator`, `spin`); the submodule pin workflow proven end to end.

### M3 交互 / Interaction — "I can use it" (M) → `v0.3.0`

| step | deliverable | size |
|---|---|---|
| M3.1 Device layer and tty | major/minor dispatch; tty with queues and line discipline (ICANON, ECHO, ISIG); serial backend first | M |
| M3.2 Real signals | numbers, masks, pending set, user handlers, handler frame and restorer, `sigaction`, `SIGKILL` / `SIGSTOP` semantics; ^C raises SIGINT | M |
| M3.3 Userland | the own libc grows a shell and coreutils (`ls`, `cat`, `cd`, `mkdir`, `rm`, `ln`, `ps`, `echo`); `ps` reads process info from a new documented extension syscall in the 1000+ range (same family as the thread/sync calls), recorded in `spec/01` §4; the 55 existing programs stay as tests | M |
| M3.4 VGA console and keyboard | VGA/CGA text console with ANSI output and scrolling; PS/2 scancodes with modifiers; console as a tty backend; ctrl-alt-del | M |
| M3.5 Sessions, groups, time | sessions, process groups, controlling tty; `utime` / `stime` accounting; `alarm`, `pause`, `nice`; wall clock: x86_64 gets our own CMOS RTC driver, riscv64 is an architecture-scoped DELTA F13 — QEMU riscv `virt` has no RTC (verified on QEMU 8.2.2: `goldfish-rtc` unavailable), so wall clock there = `stime`-set boot epoch + jiffies | S |

**Gate.** A scripted interactive session (shell reads a line, runs a program, ^C interrupts it) runs unattended in CI; a manual walkthrough with a screendump artifact.
**Milestone F.** This is the "I can use it, and it feels like 0.11" milestone. v0.3.0 is a self-contained showcase arc and the planned degradation point when owner bandwidth is squeezed.

### M4 内存 / Memory (M) → `v0.4.0`

| step | deliverable | size |
|---|---|---|
| M4.1 Copy-on-write `fork` | COW page tables; write-fault breaking; shared executable pages | M |
| M4.2 Demand paging | lazy frame allocation, stack growth, page cache for file-backed pages | M |
| M4.3 Real `mmap` / `munmap` / `brk` | correct semantics on top of M4.1 and M4.2, including shared pages | M |
| M4.4 OOM policy and accounting | explicit out-of-memory behaviour; per-process memory accounting; stress tests | S |

**Gate.** Fork/COW isolation tests, `mmap` tests, memory accounting, and a stress run that deliberately exhausts memory. The gate artifacts also record syscall round-trip, context-switch and page-fault latency — measurement only, no optimisation target, consistent with the performance non-goal in `spec/00` §4.

### M5 双架构 / Two architectures (L, split before starting) → `v0.5.0`

**Before M5 starts**, two ADRs are written: the x86_64 block-device bus and the interrupt controller. Recommended selections: v1.0 uses ATA PIO at the legacy 0x1F0/0x3F7 ports (QEMU `pc` machine compatibility mode, hardcoded ports, no PCI enumeration) and the 8259 PIC; PCI enumeration, virtio-pci, AHCI and APIC are post-v1.0 candidates (§6).

| step | deliverable | size |
|---|---|---|
| M5.1 Configuration | `config/<board>.toml` → generated consts; two board configurations build and boot | M |
| M5.2 Workspace split | the kernel crate splits into workspace crates; kernel package renamed `linur`; host tests for extracted crates | M |
| M5.3 HAL for real | the M1.5 seam generalised against two implementations; no riscv assumptions left in generic code | M |
| M5.4 x86_64 full port | boot chain, interrupts (8259 PIC + PIT, ADR-pinned), timer, ring 3, per-arch syscall table, paging, ATA PIO block device (legacy 0x1F0/0x3F7 ports, ADR-pinned), VGA console | L (split into sub-steps) |
| M5.5 riscv64 boot path ownership | build RustSBI in CI or write our own M-mode shim, so the boot path stops being a vendored third-party binary; retire `sbi-rt` (hand-rolled ecall wrappers) | M |

**Gate.** The same userland suite green on both architectures in CI; the riscv boot path produced by our own pipeline.

### M6 对齐 0.11 / Parity (M) → `v1.0.0`

| step | deliverable | size |
|---|---|---|
| M6.1 Syscall gap fill | `ioctl`, `fcntl`, `access`, `utime`, `times`, `uname`, `sync`, `mknod`, `chroot`, `mount` of a second filesystem, and the rest of the 0.11 set | M |
| M6.2 Devices | `/dev/null`, `/dev/port`, memory devices with honest behaviour; pipes with `SIGPIPE` | S |
| M6.3 Behaviour audit | sessions, signals, exec, timer and error semantics walked against 0.11; each difference either fixed or recorded | M |
| M6.4 Checklist walk | every row of `spec/03` becomes DONE, DELTA (with a reason and a fidelity-delta id) or DEFERRED (with a reason) | S |
| M6.5 Hardening | specs current; no Tier-2 dependency without a removal issue; the demo script walks the full 0.11 experience; tag annotated with the run; syscall round-trip, context-switch and page-fault latency recorded into the gate artifacts (measurement only, no optimisation target) | S |

**Gate.** `spec/03` has no unexplained gaps, on both architectures. That is the v1.0 definition.

## 4. Parallelism

Steps are sequential in the plan, but these have disjoint write scopes and may run concurrently: M2.6 and M2.7 (their own repositories) alongside M2.4 and M2.5; M3.4 (console) alongside M3.5 (sessions); M5.1 (config) alongside M5.3 (HAL) and alongside M3; M5.2 (workspace split) alongside M4; M5.3 (HAL audit) pays down debt incrementally from M3 onward. Milestone tag order and gates are unchanged — only step execution order is relaxed. The gate is always run on the merged result by the owner.

## 5. Module extraction policy (submodules)

* Extract only when the crate has its own consumers or its own CI. `easy-fs` (M2.7) and the C allocator (M2.6) qualify; nothing else does yet.
* `elf/` (M2.5) is built in-tree in the extractable shape (no_std core, host tests, no dependencies) but is **not** extracted now: it has exactly one consumer, the kernel. It is a candidate for `linur-io/elf` once the rule above is met — a second consumer or its own CI — and after the pin workflow is proven by M2.6/M2.7; not a commitment.
* Repository naming `linur-io/<module>`; the mount point is the clean module path (`fs/`, `alloc/`).
* A change spanning kernel and module is two commits plus a pin bump — the module lands first.
* CI must run `git submodule update --init --recursive`; a stale pin is a silent failure mode, so the gate asserts the pinned commit matches the built artifact.
* Agents work one repository per task; never a submodule and its parent in the same step.
* A path override is documented so a module can be edited in-tree without pinning.

## 6. Post-v1.0 candidates (not commitments)

Own M-mode shim for real hardware; ATA and a real-hardware path; PCI enumeration, virtio-pci, AHCI, APIC; MINIX v1 image compatibility and 0.11 a.out support (the L2/L3 retro stages); busybox or musl as an external oracle; `/proc`; SMP; networking; a second filesystem; `clone` and `futex` replacing the custom thread syscalls.

## 7. Revision history

| version | change |
|---|---|
| v0.1 | 15 tagged stages, S0–S14 |
| v0.2 | restructured into six milestones with day-sized steps; tags and posts moved to milestones; added debug/observability, the x86_64 smoke bring-up, userland, riscv boot-path ownership and the parity-checklist contract; M1 merges the former hygiene and ABI stages |
| v0.3 | round-4 revisions: M1.3 split (fault-safe copies / errno + userland migration); per-architecture gate capability matrices; compile-only x86_64 CI from M1.5; M2.1/M2.2 rescoping (file syscalls land once on the real VFS); Tier-2 retirement assignments (`log`→M1.2, `spin`→M2.4, `sbi-rt`→M5.5); x86_64 device ADRs (ATA PIO + 8259 PIC) before M5; M5 steps interleaved with M3/M4; latency recording in the M4/M6 gates; v0.3.0 declared the planned degradation point; rule 7 now points at `spec/01` Appendix A |
