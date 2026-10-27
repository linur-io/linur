# linur — the own ELF loader (M2.5)

Status: interface contract for M2.5. Read with `spec/02` (M2.5), `spec/01` §3/§5
(F2), `spec/03` §3/§8/§9, `docs/vfs.md` (exec) and `docs/perm.md` (the EACCES rule
for a non-regular image). M2.5 replaces `xmas-elf` with an own parser, adds
`#!` scripts and `envp`, and makes every malformed image fail with `ENOEXEC`.

The parsing half lives in a new in-tree crate `elf/` so it can be tested on the
host (D5's shape); the mapping half stays in the kernel, because it is process
policy (page tables, user stack, address-space limits). Extraction to
`linur-io/elf` is deliberately **not** part of this step (see §8).

## 1. Scope

In: the `elf/` crate (no_std, no dependencies, zero-copy) and its host-test
corpus; `MemorySet::from_elf` as a thin adapter over it; `xmas-elf` and `zero`
leaving the graph; `#!` scripts (one interpreter line, bounded recursion);
`envp` through `execve` and a user-visible `environ()`; `ENOEXEC` for every
malformed image.

Out: PIE/ASLR, `PT_INTERP` and dynamic linking (an image with one is `ENOEXEC`),
`a.out`/MINIX images (F9, L3 later), the x86_64 machine (M5.4; the parser exposes
`machine()` and the kernel checks its own), core dumps, and per-segment
`p_align` handling (every image our toolchain emits has `p_align` 4096, and the
loader ignores the field; documented limitation).

## 2. The `elf/` crate

`elf/`, package `elf`, edition 2024, `#![no_std]`, `#![deny(missing_docs)]`,
**no dependencies and no allocator** (the parser borrows the image). Host tests
live in `elf/tests/corpus.rs`, which cargo compiles as a std test binary — this is
the std adapter for now; when the crate is extracted it moves to its own adapter
crate (D5), and the parse layer does not change.

```rust
/// Why an image was refused. Every variant maps to ENOEXEC in the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    NotElf,               // no 0x7f ELF magic
    UnsupportedClass,     // EI_CLASS != 2 (ELF64)
    UnsupportedEncoding,  // EI_DATA != 1 (little endian)
    Truncated,            // shorter than the 64-byte ELF header
    BadHeader,            // e_version, e_ehsize, e_phentsize, e_phoff ...
    BadProgramHeaders,    // the phdr table lies outside the image
    BadSegment,           // filesz > memsz, offset+filesz > len, address overflow
}

/// One PT_LOAD segment, borrowing the image bytes.
pub struct Segment<'a> {
    /// Destination virtual address (p_vaddr).
    pub vaddr: usize,
    /// Memory size; the loader zero-fills the tail beyond data.len() (p_memsz).
    pub memsz: usize,
    /// The p_filesz file bytes to copy at vaddr.
    pub data: &'a [u8],
    /// p_flags has PF_R.
    pub read: bool,
    /// p_flags has PF_W.
    pub write: bool,
    /// p_flags has PF_X.
    pub execute: bool,
}

pub struct Elf<'a> { /* private */ }

/// Parse and validate an ELF64 little-endian image (docs/elf.md §2).
pub fn parse(data: &[u8]) -> Result<Elf<'_>, ElfError>;

impl<'a> Elf<'a> {
    /// e_machine: 243 is RISC-V, 62 is x86_64.
    pub fn machine(&self) -> u16;
    /// e_type: 2 is ET_EXEC, the only kind the kernel runs.
    pub fn kind(&self) -> u16;
    /// e_entry, the first instruction executed.
    pub fn entry(&self) -> usize;
    /// Whether the image names an interpreter (`PT_INTERP`): dynamically linked.
    pub fn has_interpreter(&self) -> bool;
    /// The loadable segments in program-header order.
    pub fn segments(&self) -> impl Iterator<Item = Segment<'a>> + '_;
}
```

Validation split — the parser owns **image structure**, the kernel owns **process
layout**:

| check | owner |
|---|---|
| magic, class 64, little endian, `e_version == 1`, `e_phentsize == 56` | parser |
| `e_phoff + e_phnum * 56 <= len` | parser |
| every `PT_LOAD`: `filesz <= memsz`, `offset + filesz <= len`, `vaddr + memsz` fits in `usize` | parser |
| `machine() == 243`, `kind() == 2`, and no `PT_INTERP` | kernel |
| at least one `PT_LOAD`, each non-empty, `end <= TRAP_CONTEXT_BASE - PAGE_SIZE - USER_STACK_SIZE`, segments do not overlap | kernel |
| mapping permissions: `U` always, plus `R`/`W`/`X` from the segment flags | kernel |

Non-`PT_LOAD` program headers are ignored. `e_entry` is not required to lie inside
a segment (a wrong entry faults at run time, as it does on Linux).

## 3. The kernel adapter

`MemorySet::from_elf` becomes: `elf::parse(data).ok()?` → machine/kind check →
segment loop with the checks above (`MapArea::new(start, end, Framed, perm)` and
`push(area, Some(segment.data))`; the zero-filled tail needs no extra work
because every frame is zeroed when it is allocated) → user stack at
`max_end + PAGE_SIZE` → `heap_bottom = program_brk = USER_HEAP_BASE`. `None` is
`ENOEXEC` at the call sites (`try_new`, `exec`, `spawn`).

## 4. `envp`

`execve` is 221 with `(path, argv, envp)`; M2.5 starts reading the third argument
(it was ignored). Both vectors are arrays of NUL-terminated strings followed by a
NULL pointer; the strings are copied through the fault-safe layer, and
`argv + envp` share the user-stack budget (`USER_STACK_SIZE`; E2BIG when the
slots plus bytes do not fit).

User stack, from the top down (`slot = 8`):

```
ustack_top - (argc+1+envc+1)*slot : the argv array then the envp array, both NULL-pointer terminated
below that                      : the argv strings then the envp strings, NUL-terminated, 8-byte aligned sp
```

The trap context carries `a0 = argc`, `a1 = &argv[0]`, `a2 = &envp[0]`. The user
library's `_start(argc, argv, envp)` builds both slices, keeps `main(argc, argv)`
unchanged, and exposes:

```rust
/// The environment this process was exec'd with, empty when none was given.
pub fn environ() -> &'static [&'static str];
```

`user_lib::exec(path, argv)` keeps its signature and passes `envp = NULL`;
`user_lib::execve(path, argv, envp)` takes two pointer arrays and is the test
entry point.

## 5. `#!` scripts

After resolving the path and reading the image, `sys_exec` follows `#!` lines
(`spawn` shares the helper; it starts from `argv = [path]`, `envp = []`):

* a script is an image whose first two bytes are `#!`;
* the interpreter line is the rest of the line up to `\n` (or 128 bytes when the
  line is longer than the buffer), trimmed; the first token is the interpreter
  path, the remainder of the line is **one** optional argument (Linux's
  `binfmt_script` semantic);
* the interpreter is resolved like any exec path (cwd-relative or absolute, the
  same permission and regular-file checks), and the new argv is
  `[interp, optional_arg?, script_path] + original_argv[1..]`; `envp` is unchanged;
* recursion is bounded at 4 levels, like Linux; deeper is `ENOEXEC` (a recorded
  divergence: Linux answers `ELOOP`);
* an empty interpreter token is `ENOEXEC`, a missing interpreter is `ENOENT`, an
  interpreter that is not a regular file is `EACCES`, and an interpreter image
  that fails to parse is `ENOEXEC`.

The script itself never reaches the page tables; only the final interpreter image
does.

## 6. Errors

No new errno. Malformed or unloadable images (`elf::ElfError`, wrong machine,
`ET_DYN`, no segment, out-of-range or overlapping segments) are `ENOEXEC`;
missing interpreter `ENOENT`; non-regular image or interpreter `EACCES`;
oversized `argv`/`envp` `E2BIG`. `syscall_errors`' existing ENOEXEC case keeps
passing.

## 7. Tests

**Host corpus** (`elf/tests/corpus.rs`, assert-based, no boot): a hand-built
ELF64 image plus one defect per case — no magic, ELF32 class, big-endian encoding,
truncated header, `e_phoff` past EOF, `e_phentsize` wrong, `filesz > memsz`,
`offset + filesz > len`, `vaddr` overflow, wrong machine, `ET_DYN`, no
`PT_LOAD`, two disjoint segments with a bss tail, a non-`PT_LOAD` header that
must be skipped, and a `PT_INTERP` image that parses and reports its interpreter
(the kernel refuses to run it).

**Kernel program** `user/src/bin/elf_test.rs` (+1 in `SUCC_TESTS`, gate 76 → 77):
crafts the defective images into regular files and asserts `exec` fails with
`ENOEXEC` for each; `#!/hello_world` and `#!/hello_world 7` run the interpreter
(fork + wait, child exit 0); `#!/no_such_interp` is `ENOENT`; and the helper
`user/src/bin/envp_child.rs` (spawned, never a suite entry) checks
`environ()` is `["A=1", "B=2"]` through `execve`, and empty through plain `exec`.

## 8. Dependencies, tooling, and the extraction stance

* `xmas-elf` and `zero` leave `os/Cargo.toml` and the graph; `DEPENDENCIES.md`
  rows go, `deny.toml` moves both names to `[bans] deny`, and the
  `RUSTSEC-2025-0018` ignore is removed with the crate it described.
* `elf` is an own Tier-0 crate: it joins `deny.toml`'s allow list, `make lint`
  (`cargo fmt --check` and `clippy --all-targets -- -D warnings`) and
  `make verify`'s host-test phase (`cargo test`).
* **Extraction stance.** The crate is written in the extractable shape (no_std
  core, host tests, no dependencies) but stays in-tree: `spec/00` D4 and
  `spec/02` §5 extract only modules with their own consumers or their own CI,
  and this one has exactly one consumer, the kernel. `spec/02` §5 records `elf`
  as a candidate to move to `linur-io/elf` once that rule is met (a second
  consumer, or a CI of its own), after the pin workflow itself has been proven by
  M2.6/M2.7. Nothing else changes when that happens except the mount.
