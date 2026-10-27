# linur — the physical frame allocator (M2.6)

Status: interface contract for M2.6. Read with `spec/02` (M2.6), `spec/01` §2 and §5
(F4), `spec/03` §2, `spec/00` §3 (D8), and `docs/block.md` for the shape a leaf-module
contract takes. M2.6 moves the physical page-frame allocator out of Rust into the C
repository `linur-io/alloc` — D8's first C module — mounted here as a submodule, and
replaces `buddy_system_allocator` in the kernel and user heaps with our own free list.

## 1. Scope

In: the C ABI and the map it walks; the Rust shim in `os/src/mm/frame_allocator.rs`;
the board's RAM constants the map is sized from; the hand-written `os/build.rs` that
compiles the module with `clang` and links the object; the submodule pin and its bump
procedure; the kernel heap (`os/src/mm/heap_allocator.rs`) and the user heap
(`user/src/lib.rs`) as the `heap` crate.

Out: virtual memory, page tables and every mapping (unchanged by this step); frame
*sharing* policy — COW, refcount bumps, demand paging (M4.1/M4.2, §9); a boot-time
allocator test program (the 77-test suite is the integration test); `lazy_static` and
the `spin 0.9.9` it pulls in (open D7 gap, `DEPENDENCIES.md`); the x86_64 kernel
(M5.4 — its allocator wiring is part of that port, not this step).

## 2. Why C, and where the line sits

D8 permits C only for leaf modules with a C ABI of fewer than ten functions,
developed and tested in their own repository, consumed through a thin `extern "C"`
shim. The frame allocator is the smallest such module: it owns no objects, no Rust
type crosses the boundary, it never dereferences a frame, and its whole state is the
caller-owned map plus two range words.

The module has no libc and needs none: it is compiled `-ffreestanding`, performs no
memory operation other than the map it is handed, and therefore references no
`memcpy`/`memset` (the kernel image still provides those for the compiler builtins).
Rust provides the memory — the map itself and the frame zeroing — and owns the lock.

## 3. The ABI

| function | contract |
|---|---|
| `void frame_init(unsigned char *map, unsigned long first, unsigned long end)` | `first`/`end` are page-frame numbers, `end` exclusive; zeroes `map[0 .. end - first)`; records the range. `first <= end` is the caller's promise. |
| `long frame_alloc(void)` | Scans **backwards** from `end - 1` to `first` (0.11's `get_free_page` order); returns the first ppn whose map byte is 0 after setting it to 1, else `-1`. |
| `int frame_free(unsigned long ppn)` | `-1` when `ppn` is outside `[first, end)` or its byte is already 0; otherwise decrements the count and returns 0. |

The map has one byte per frame **indexed by `ppn - first`**, so its size is the
allocatable window, not the physical address space: 32 KiB today (§4). A byte is an
owner count, not a flag: 0 is free, 1 is owned, and 2 is already a legal value for a
shared frame — M4.1 adds the two calls that raise and lower it at a new submodule tag
without touching this shape.

The shim is the only caller and the only place the C is trusted: `frame_alloc()`
still returns `Option` and its user still `unwrap`s (unchanged from M1), and
`frame_free` returning `-1` panics with the same message the stack allocator used,
because an invalid free is an allocator bug, not a recoverable error.

## 4. Memory layout and the lock

`FRAME_MAP` is a `[u8; (MEMORY_END - MEMORY_START) / PAGE_SIZE]` kernel `.bss` array —
32 KiB for QEMU `virt` (RAM `[0x8000_0000, 0x8800_0000)`, 32,768 frames). It lives below
`ekernel`, and the allocator's window starts at `ceil(ekernel)`, so the map can never
be handed out: `FRAME_MAP ⊂ [0, ekernel)` holds by construction, and `spec/01` §2's
"stack allocator over `[ekernel, 0x8800_0000)`" becomes the same window with the
bookkeeper in C.

`MEMORY_START` is new board knowledge (`os/src/boards/qemu.rs`). A map covering the
whole address space instead would waste 512 KiB of `.bss` on the hole below RAM, and
indexing from RAM base rather than from `first` keeps the array a compile-time
constant — `ekernel` is a link symbol, not a constant.

Locking stays in Rust: the C module is not reentrant and has no lock. The shim takes
`SpinMutex<()>` (`os/src/sync/spinlock.rs`) around every call, exactly as M2.4's block
layer does; traps run with interrupts disabled and the scheduler switches only at the
end of a trap, so a call in a syscall cannot be preempted mid-allocation. Frame
zeroing stays in `FrameTracker::new`; moving it into C would buy nothing and would
make the module touch memory it does not own.

## 5. Build integration

`os/build.rs` (hand-written; no `cc` crate) does three things: finds a compiler,
compiles `../alloc/src/frame.c` for cargo's `TARGET`, and hands the object to the
linker.

- Compiler: `CLANG` if set, else `clang`, else `clang-19` … `clang-22`. The name
  `clang` is not guaranteed — this host ships `clang-19`/`clang-22` only. A missing
  compiler is a build-script error that says how to install one; `make preflight`
  repeats the check before cargo runs.
- Flags, identical on both targets: `-std=c17 -O2 -Wall -Wextra -Werror
  -ffreestanding -fno-stack-protector -fno-pic -I../alloc/include`. riscv64 adds
  `--target=riscv64-unknown-elf -march=rv64gc -mabi=lp64d -mcmodel=medany`; x86_64
  adds `--target=x86_64-unknown-none -mno-red-zone`.
- Link: `cargo:rustc-link-arg=<OUT_DIR>/frame.o`. rust-lld places the object's
  `.text`/`.bss` through `linker-qemu.ld` like any other input; no archive is built
  and no crate enters the dependency graph.
- Rebuild triggers: `rerun-if-changed=../alloc` (the whole submodule) and
  `rerun-if-env-changed=CLANG`.

Only cargo's target is compiled by the kernel build. The **other** target is compiled
by the module's own CI (`make cross`), which keeps a useless x86_64 object out of
every kernel build while still proving the module freestanding on both architectures;
when M5.4 links an x86_64 kernel, the same build script produces the object for
`$TARGET` with no change.

## 6. The submodule pin

`alloc/` is a submodule of `linur-io/alloc` at the repository root, pinned to tag
`v0.1.0`. `.gitmodules` uses the **HTTPS** URL so that CI's `actions/checkout` with
`submodules: recursive` needs no SSH key; the maintainer's checkout still pushes over
SSH from the submodule's own remote.

The pin workflow this step proves end to end:

1. in `linur-io/alloc`: commit, tag `v0.x.y`, push the tag;
2. in `linur`: `git -C alloc fetch --tags && git -C alloc checkout v0.x.y`;
3. `git add alloc` (the gitlink) and commit with the reason for the bump.

A clone without submodules fails in `build.rs` with a message naming
`git submodule update --init`, not with a linker error.

## 7. Tests

The C module carries its own assert-based host test binary (`test/frame_test.c`, run by
`make test` in that repository and by its CI with `clang` and `gcc`): the backwards
order, reuse after free, out-of-range and double free, a shared frame's count reaching
0, re-init, exhaustion of a 1024-frame range, and a randomized soak against a
reference model.

On this side the integration test is the existing gate: every process, page table,
trap context and eager `fork` in the 77-test suite allocates and frees frames through
the C module, and `make verify` boots it. The two heaps have host tests of their own in
`heap/tests/` (reuse, coalescing, alignment, exhaustion, randomized soak), which
`verify.sh` runs next to `elf`'s.

## 8. Removal ledger

`buddy_system_allocator` leaves `os/Cargo.toml` and `user/Cargo.toml` in this step
together with its `DEPENDENCIES.md` row, its `Cargo.lock` entries, `spin 0.7.2` (its
only user), and `deny.toml` gains a ban so it cannot return. `spin 0.9.9` remains
through `lazy_static`'s `spin_no_std` feature — the open D7 gap already recorded, not a
regression.

## 9. Extension point (M4.1)

COW needs `frame_get(ppn)` (raise the count, return it) and `frame_put(ppn)` (lower it,
report whether the frame is now free). Both are a few lines over the same map and land
as a new submodule tag; `frame_free` already has the decrement shape they need.
Nothing in this contract changes.
