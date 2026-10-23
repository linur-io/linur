# linur — the re-landed lab calls and the stride scheduler (M2.1)

Status: interface contract for M2.1. Read with `spec/02` (M2.1), `spec/01` §4 and
Appendix A (the reference commits) and `docs/errors.md` (the errno ABI). The port is
by semantics, not a cherry-pick: the reference diffs target the ch3–ch5 trees and
only say what the features did.

## 1. Scope

In: `brk`, `mmap`, `munmap`, `dup2`, `spawn`, `sys_trace`, the stride scheduler
and `set_priority`; the recovered lab programs become the seed regression suite
(`spec/02` M2.1), and the stride scheduler is the final one (DELTA F14).

Out: `stat`/`fstat`/`link`/`unlink`/`lseek` (M2.2, on the real VFS), COW and demand
paging (M4.1/M4.2), real `mmap` semantics with fd/offset/shared pages (M4.3), and the
explicit OOM policy (M4.4).

## 2. New syscall numbers

Numbers keep the M1 ABI: Linux riscv64 numbers where the call exists there, and the
1000+ extension family otherwise (`spec/01` §4). The thread/sync set is 1000–1032;
M2.1 continues at 1040.

| number | call | arguments | success | errors |
|---|---|---|---|---|
| 214 | `brk` | `addr` | the new break | `ENOMEM` |
| 215 | `munmap` | `start`, `len` | 0 | `EINVAL` |
| 222 | `mmap` | `start`, `len`, `prot` | 0 | `EINVAL` |
| 1040 | `spawn` | `path` | the child pid | `ENOENT`, `ENAMETOOLONG`, `ENOEXEC` |
| 1041 | `trace` | `request`, `id`, `data` | request-dependent (§6) | `EINVAL`, `EFAULT` |
| 1042 | `set_priority` | `priority` | the priority | `EINVAL` |
| 1043 | `dup2` | `oldfd`, `newfd` | the new descriptor | `EBADF` |

`dup2` has no riscv64 Linux number (Linux keeps only `dup3` there), so it takes an
extension number; `spawn`, `trace` and `set_priority` are linur extensions and have
no Linux counterpart at all. `ENOMEM` (−12) joins the errno set in `docs/errors.md` §6.

## 3. `brk` — the heap

`USER_HEAP_BASE` (`config.rs`, `0x4000_0000`) is a fixed user virtual address, away
from the image (`0x1_0000`), the user stacks and trap contexts (top of the user
address space) and the addresses the mmap tests use (`0x1000_0000`). The heap starts
empty there and grows upward eagerly.

* `brk(0)` returns the current break and changes nothing.
* `brk(addr)` sets the break to `addr`: it maps the pages up to `addr` when growing
  and frees them when shrinking, then returns `addr` (not page-rounded).
* `ENOMEM` when `addr` is below `USER_HEAP_BASE`, above `USER_ADDR_TOP` (the
  `1 << 38` user-address ceiling — `VirtAddr` keeps only the SV39 low half, so a
  higher address would name kernel space), or when the new page range would overlap
  another user mapping (an `mmap` region, a user stack, a trap context). The check
  runs before any page is mapped, so a refused call changes nothing.
* Pages are allocated eagerly, as everywhere in M1 (`spec/01` §5 F3). A request that
  exhausts physical memory therefore panics through `frame_alloc().unwrap()`; that is
  the known limitation the M4.4 OOM policy removes, not an M2.1 task.
* M4.3 replaces the eager grow with demand paging; the user-visible contract above
  stays.

## 4. `mmap` / `munmap` — anonymous fixed-address mappings

M2.1 is the lab's provisional form: anonymous, fixed address, eager, three arguments.
M4.3 makes it the six-argument Linux call with fd/offset and shared pages.

`mmap(start, len, prot)`:

* `start` must be page-aligned, `prot` must be a non-empty subset of
  `PROT_READ`=1, `PROT_WRITE`=2, `PROT_EXEC`=4, `start + len` must not overflow, and
  `start + len` must not exceed `USER_ADDR_TOP`; otherwise `EINVAL`. `len == 0`
  succeeds and maps nothing.
* Every page in the range must be unmapped today; a conflict with any existing
  mapping is `EINVAL`. `prot` bits map straight onto PTE R/W/X plus U. A write-only
  mapping (`PROT_WRITE`, no read) is accepted, as on RISC-V it simply faults on any
  access — the mmap2 regression depends on that. An `mmap` over `USER_HEAP_BASE` is
  accepted while the heap is still empty, but it makes later `brk` growth into that
  range fail with `ENOMEM`.
* `munmap(start, len)`: `start` page-aligned, no overflow, no address above
  `USER_ADDR_TOP`, `len == 0` succeeds; every page in the range must be mapped or the
  call returns `EINVAL` *without unmapping anything* (validate first, then unmap).

Address-space model, unchanged from the lab: a `MapArea` may be left with holes by
`munmap`. Two consequences are part of this contract: `fork` copies only the pages
that are mapped (so a child does not materialise the parent's holes), and shrinking a
region skips holes instead of asserting on them.

## 5. `dup2`

`dup2(oldfd, newfd)` makes `newfd` refer to the same open file as `oldfd`:

* `oldfd` must be an open descriptor, else `EBADF`.
* `oldfd == newfd`: nothing happens, return `newfd`.
* `newfd` within the table: the previous file, if any, is closed and replaced.
* `newfd == fd_table.len()`: the table grows by one (the descriptor Linux would
  allocate when the caller asks for the next free one).
* `newfd > fd_table.len()`: `EBADF`. Linux allows skipping ahead under `RLIMIT_NOFILE`;
  linur has no rlimit and refuses to allocate an unbounded hole from a user argument.
  M2.2's fd-table work may revisit this.

## 6. `trace`

`trace(request, id, data)` is the teaching/debug facility, kept as an extension:

| request | meaning | result |
|---|---|---|
| 0 | read one byte at user address `id` | 0–255 |
| 1 | write the low byte of `data` at user address `id` | 0 |
| 2 | call count of syscall `id` in the calling task | the count |

* User memory is reached through the fault-safe copy layer, so a bad address is
  `EFAULT`, never a kernel panic.
* Any other request is `EINVAL`.
* Counts are per task (`TaskControlBlock`), start at 0 for a new task, survive
  `exec` (the task continues), and are incremented at dispatch entry for every call —
  including `trace` itself and unknown ids that return `ENOSYS`. A `trace` request 2
  therefore counts the call asking the question. This matches the reference test.

## 7. `spawn`

`spawn(path)` creates a new process from the executable at `path` in `fs.img`, with an
empty argument list, and returns its pid. The child is a child of the calling process
(`waitpid` reaps it) and starts as a fresh address space with the standard streams
(fd 0 standard input, 1 and 2 console output), like any new process; the caller keeps
its own.
`ENOENT` when the file does not exist, `ENAMETOOLONG` for a name longer than one
directory entry, `ENOEXEC` when the image does not load — a bad image is an error
return, not the `init` process's panic.

## 8. Stride scheduler and `set_priority`

The FIFO ready queue is replaced by a stride scheduler, which is the final algorithm
(DELTA F14: fairness, not 0.11's counter refill):

* Each task has a `priority` (default `DEFAULT_PRIORITY` = 16) and a `stride` (0).
* Every time a running task leaves the CPU through the scheduler (`yield`, a timer
  tick; `sleep`'s blocking path charges nothing because it is not descheduled by
  `suspend`), it is charged `pass = BIG_STRIDE / priority`, at least 1, saturating at
  `isize::MAX`.
* The next task is the ready task with the smallest stride; equal strides are broken
  by pid, smaller first, so the order is deterministic.
* `set_priority(priority)` sets the calling task's weight and returns it. `priority <
  MIN_PRIORITY` (2) is `EINVAL`. Weights above `BIG_STRIDE` give `pass` 1.
* `BIG_STRIDE` (240) fixes the resolution: with the default weight, one charge is
  worth 15 stride units. It lives in `config.rs` beside the other scheduler
  constants.

## 9. Seed regression suite

The recovered lab programs are added to `user/src/bin` and to the harness:

* success: `mmap0`, `mmap3`, `unmap`, `unmap2`, `brk_test`, `dup2_test`,
  `spawn0`, `spawn1`, `ch5_setprio`, `ch5_stride`, `trace_test`;
* killed by SIGSEGV (−11): `mmap1`, `mmap2`;
* spawned helpers, not run standalone: `ch5_exit0`, `ch5_exit1`, `ch5_getpid`,
  `ch5_stride0`–`ch5_stride5`.

`brk_test`, `dup2_test` and `trace_test` are the new-coverage programs: the reference
commits shipped the others but no `brk` or `dup2` test. `verify.sh` expects 64 tests,
and every added program is listed in `usertests.rs` in exactly one place.
