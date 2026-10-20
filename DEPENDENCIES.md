# Dependency ledger

One row per third-party crate in the `os`, `user` and `easy-fs` dependency graphs —
direct and transitive, including the proc-macro crates compiled for the host while
building them — with versions as pinned in the tracked `Cargo.lock` files. Tiers are D7
(`docs/spec/00-requirements.md` §3):

* **Tier 0 — never third-party.** The claim of the project: physical and virtual
  memory, page tables, trap and context switch, scheduler, VFS, on-disk FS and buffer
  cache, device drivers, ELF loader, libc, shell/coreutils, boot path. A row whose
  purpose covers one of those is a violation awaiting its removal milestone, and is
  marked as such below.
* **Tier 1 — permanently allowed** (architecture wrappers, `bitflags`, `cfg-if`).
* **Tier 2 — temporary**, each with the milestone step that retires it. Removal deletes
  the crate from `Cargo.toml` and its row from this file in the same PR (roadmap §1
  rule 6), and adds it to `deny.toml`'s `[bans] deny` list so it cannot return.

A crate that D7 does not name inherits the tier of the crate that pulls it in, because
it cannot be retired on its own. `deny.toml` carries the same crate names in its
`[bans] allow` set (plus our own Tier-0 crates, which cargo-deny also requires to be
listed), so "no crate without a row here" is machine-checked; versions live only in the
tracked lock files and in this table.

Two Tier-1 entries of D7 have no row because nothing pulls them in yet: `x86_64` (the
port starts at M1.5) and `cfg-if` (reached only from host-only tooling today).

## Kernel-image graph

| crate | version | purpose | tier | owner | removal |
|---|---|---|---|---|---|
| bitflags | 1.3.2 | `bitflags!` types for signal and open-flag sets | Tier 1 | Fuxi | — |
| buddy_system_allocator | 0.6.0 | kernel heap and user heap allocator — **Tier-0 violation** kept from rCore | Tier 2 | Fuxi | M2.6 |
| critical-section | 1.2.0 | critical-section abstraction required by `riscv` | Tier 1 | Fuxi | — |
| embedded-hal | 1.0.0 | delay and serial traits required by `riscv` | Tier 1 | Fuxi | — |
| lazy_static | 1.5.0 | global singletons, via the `spin_no_std` feature | Tier 2 | Fuxi | open — D7 gap, owner decision (proposed: retire with `log` in M1.2) |
| log | 0.4.33 | log macros behind `os/src/logging.rs` | Tier 2 | Fuxi | M1.2 |
| pastey | 0.2.3 | `paste!` for register definitions in `riscv` | Tier 1 | Fuxi | — |
| proc-macro2 | 1.0.107 | proc-macro plumbing, host-side at build time | Tier 1 | Fuxi | — |
| quote | 1.0.47 | proc-macro plumbing, host-side at build time | Tier 1 | Fuxi | — |
| riscv | 0.16.1 | architecture register and CSR wrappers | Tier 1 | Fuxi | — |
| riscv-macros | 0.4.1 | derive macros for the `riscv` register wrappers | Tier 1 | Fuxi | — |
| riscv-types | 0.1.0 | register and CSR types for `riscv` | Tier 1 | Fuxi | — |
| sbi-rt | 0.0.2 | SBI calls for console output, timer and shutdown | Tier 2 | Fuxi | M5.5 |
| sbi-spec | 0.0.4 | SBI extension constants for `sbi-rt` | Tier 2 | Fuxi | M5.5 |
| spin | 0.7.2 | spinlock in the easy-fs block cache and in `buddy_system_allocator` | Tier 2 | Fuxi | M2.4 |
| spin | 0.9.9 | spinlock inside `lazy_static`'s `spin_no_std` | Tier 2 | Fuxi | open — same D7 gap as `lazy_static` |
| static_assertions | 1.1.0 | compile-time asserts in `sbi-spec` | Tier 2 | Fuxi | M5.5 |
| syn | 2.0.119 | procedural-macro parsing, host-side at build time | Tier 1 | Fuxi | — |
| unicode-ident | 1.0.24 | identifier tables for `proc-macro2` and `syn`, host-side at build time | Tier 1 | Fuxi | — |
| virtio-drivers | 0.1.0 (`rev 4ee80e5`) | virtio-MMIO block driver — **Tier-0 violation** | Tier 2 | Fuxi | M2.4 |
| volatile | 0.3.0 | volatile MMIO access in `virtio-drivers` | Tier 2 | Fuxi | M2.4 |
| xmas-elf | 0.7.0 | ELF parsing in `exec` — **Tier-0 violation** | Tier 2 | Fuxi | M2.5 |
| zero | 0.1.3 | `Zero` trait bounds used by `xmas-elf` | Tier 2 | Fuxi | M2.5 |
| bootloader/rustsbi-qemu.bin | RustSBI 0.3.1, 32 KiB | S-mode firmware the kernel boots under — **Tier-0 violation**; a vendored binary, not a crate, so it has no `deny.toml` entry | Tier 2 | Fuxi | M5.5 |

**Checks.** `cargo deny check licenses bans sources advisories` is green for `os`,
`user` and `easy-fs` with cargo-deny 0.20.2. The only advisory in the graph is
RUSTSEC-2025-0018 against `xmas-elf` (out-of-bounds read on a malformed ELF); it is
ignored in `deny.toml` until M2.5 replaces the parser, since no untrusted image reaches
`exec` before then. `virtio-drivers` still has no `license` field of its own, so the
license check reports it as a warning rather than an error.

## Host-only tooling

`easy-fs-fuse` is a host binary that packs `fs.img` and never runs inside an image. Its
`clap` 2.34.0 and `rand` 0.8.6 closure (libc, getrandom, rand_chacha, ppv-lite86,
zerocopy, ansi_term, atty, textwrap, winapi, cfg-if, …) is therefore outside the D7
claim and is not a row in this ledger. `deny.toml`'s allow list targets `os`, `user` and
`easy-fs` for the same reason.