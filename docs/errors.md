# linur — syscall errors, fault-safe copies, and the errno ABI (M1.3a/M1.3b)

Status: interface contract for M1.3a and §6 for M1.3b. Read with `spec/02` (M1.3a deliverable) and
`spec/01` §3, §6. The full Linux-shaped ABI — six arguments, per-arch tables,
errno for every call — is M1.3b; this step only removes the ways a user program
can panic the kernel with a bad argument.

## 1. What changes

* A user pointer is validated before the kernel dereferences it. An invalid
  argument produces a negative error return instead of a panic.
* M1.3a introduced three error numbers (M1.3b added the rest, §6):

| name | value | returned when |
|---|---|---|
| `ENOSYS` | −38 | the syscall id is unknown |
| `EINVAL` | −22 | known call, invalid argument: unknown `open` flag bits, invalid sync-object id, malformed executable, `argv` larger than the user stack, `fork`/`exec` from a multithreaded process |
| `EFAULT` | −14 | the pointer range is not mapped, not user-accessible, or not writable where the kernel writes |

* M1.3a kept the `-1` / `-2` sentinels (`waitpid` −1 no such child, −2 not
  exited yet; `waittid`; `open`; `kill`; `-0xdead` deadlock). M1.3b replaced
  all of them with errno (§6), together with the userland migration.

## 2. Copy layer — `os/src/mm/user.rs`

```rust
/// Copy `dst.len()` bytes from the user address space of `token`.
pub fn copy_from_user(token: usize, dst: &mut [u8], src: *const u8) -> Result<(), isize>;
/// Copy `src` into the user address space at `dst`.
pub fn copy_to_user(token: usize, dst: *mut u8, src: &[u8]) -> Result<(), isize>;
/// User pages the kernel reads (file write, path, argv).
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Result<UserBuffer, isize>;
/// User pages the kernel writes (file read).
pub fn translated_byte_buffer_mut(token: usize, ptr: *mut u8, len: usize) -> Result<UserBuffer, isize>;
/// A NUL-terminated user string, copied out of user memory.
pub fn translated_str(token: usize, ptr: *const u8) -> Result<String, isize>;
```

The error is the value the syscall returns, so a call site is
`let buf = match translated_byte_buffer(token, ptr, len) { Ok(b) => b, Err(e) => return e };`.

Rules:

1. A zero-length range always succeeds, even at an invalid address: `read`/`write`
   of zero bytes returns 0, and no page is touched.
2. A range is validated page by page **before** any byte is touched. Every page
   must be mapped, must carry the user bit, and — when the kernel writes — the
   writable bit. The first page that fails aborts the whole call with `EFAULT`;
   nothing is partially copied. A length that overflows the address space is
   `EFAULT`.
3. Kernel-only mappings (kernel text/data, the trampoline, the per-thread trap
   context page) have no user bit and are therefore unreachable from a syscall,
   even when they share the user page table.
4. No S-mode page fault is possible on the user-memory path: a range that fails
   validation is never dereferenced. Traps taken in S-mode still panic
   (`trap_from_kernel`), and that panic now means a kernel bug, not a user error.

Why software validation instead of an exception fixup table: on riscv64 the page
table is the authoritative map, so validation is exact and cheap; a fixup table
becomes necessary only when a translation can fail for reasons the page table
does not show — demand paging (M4.2) and COW write faults (M4.1).

## 3. Panic paths removed

Every row is reachable from user space and used to kill the kernel.

| # | path | old | now |
|---|---|---|---|
| 1 | unknown syscall id (`syscall/mod.rs`) | `panic!` | `ENOSYS` |
| 2 | bad user pointer in any copy helper (`mm/user.rs`) | `unwrap()` | `EFAULT` |
| 3 | unknown `open` flag bits (`syscall/fs.rs`) | `from_bits().unwrap()` | `EINVAL` |
| 4 | invalid mutex/semaphore/condvar id (`syscall/sync.rs`) | index + `unwrap()` | `EINVAL` |
| 5 | `waittid` with an out-of-range tid (`syscall/thread.rs`) | index | −`ESRCH` |
| 6 | `read(0, buf, len != 1)` (`fs/stdio.rs`) | `assert` | one-byte partial read; `len == 0` returns 0 |
| 7 | `write(1, non-UTF-8)` (`fs/stdio.rs`) | `from_utf8().unwrap()` | bytes written verbatim |
| 8 | `fork`/`exec` from a multithreaded process (`process.rs`) | `assert` | `EINVAL` |
| 9 | `exec` of a malformed or non-ELF file (`mm/memory_set.rs`) | `unwrap()`/`assert` | `−ENOEXEC` |
| 10 | `exec` with `argv` larger than the user stack (`task/process.rs`) | stack underflow panic | `−E2BIG` |
| 11 | `open`/`link` with a name longer than the on-disk limit (`easy-fs`) | slice panic | `−ENAMETOOLONG` (the real VFS keeps it at M2.2) |
| 12 | `ebreak` in user mode (`trap/mod.rs`) | "unsupported trap" panic | `SIGILL` |
| 13 | double unlock of a blocking mutex (`sync/mutex.rs`) | `assert` | no-op |
| 14 | unlock/up of an object the caller never held, including a negative detector count (`task/process.rs`) | index panic / underflow | no accounting change |

## 4. Deliberately still fatal, or deferred

Resource exhaustion is a policy question, not an argument-validation one, and it
has owners elsewhere: frame allocation and kernel-heap exhaustion → M4.4; disk
full and the 32-bit file-size ceiling → M2.2/M2.4; a trap taken in S-mode is a
kernel bug and must stay a panic.

## 5. Negative tests

`user/src/bin/syscall_errors.rs`, registered in `usertests` `SUCC_TESTS` (a
passing program: it asserts the error values and exits 0). It covers `ENOSYS`
(unknown id), `EFAULT` (unmapped read/write/pipe/exec pointer, the kernel-only
trampoline, a read-only user page for a kernel write), `EINVAL` (unknown `open`
flags, invalid sync ids, malformed ELF, `fork` and `exec` from a thread) and the
no-panic cases (`waittid` out of range, over-long file name, releasing an unheld
mutex or semaphore). Each case prints `syscall_errors: ok <name>` or `FAIL`; the
tally is the program's exit code.

## 6. M1.3b — the errno ABI, six arguments, per-arch tables

The kernel boundary is Linux's convention: a call returns a non-negative result
on success and `-errno` on failure. The `-1`/`-2` sentinels of §1 are gone.

**Arguments.** Up to six: `a0`–`a5` (`x10`–`x15`), number in `a7` (`x17`),
result in `a0`. The handlers are the shared semantic core; the number→handler
table is per architecture — `os/src/syscall/table.rs` holds the riscv64 table
(x86_64 numbers differ and arrive with the M5.4 port).

**Error values.**

| value | name | produced by |
|---|---|---|
| −2 | `ENOENT` | `open`/`exec`: no such file; `open("")` |
| −3 | `ESRCH` | `kill`/`waittid`: no such process or thread |
| −7 | `E2BIG` | `exec`: `argv` does not fit the user stack |
| −8 | `ENOEXEC` | `exec`: not a loadable ELF image |
| −9 | `EBADF` | `read`/`write`/`close`/`dup`: bad descriptor |
| −10 | `ECHILD` | `waitpid`: no such child |
| −11 | `EAGAIN` | `waitpid`/`waittid`: child or thread has not exited |
| −14 | `EFAULT` | the pointer range is not accessible (see §2) |
| −22 | `EINVAL` | bad flag bits, object id, signal, or multithreaded `fork`/`exec` |
| −35 | `EDEADLK` | the deadlock detector refuses the request |
| −36 | `ENAMETOOLONG` | a name longer than one directory entry |
| −38 | `ENOSYS` | unknown syscall number |

**`waittid` takes a status pointer**, like `waitpid`: `waittid(tid, int *status)`
returns the tid. An exit code is therefore never the syscall result, so a
negative status (the thread was killed by a signal) cannot be mistaken for an
errno — `-EAGAIN` is 11 and `-SIGSEGV` is also 11.

**libc layer.** The kernel returns `-errno`; `user_lib` returns POSIX: `-1` on
failure and the reason in `errno()`. That is why most of the 55 programs needed
no change, and it is the layer the migration touched: the `wait`/`waitpid`/
`waittid` wrappers loop on `EAGAIN` and return `-1` otherwise; the three ch8
deadlock tests test `errno() == EDEADLK`. `errno` is one global until the real
per-thread libc (M3.3).
