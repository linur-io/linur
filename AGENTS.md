# Repository Guidelines

## Project Structure & Module Organization

`linur` (LINUx + Rust) is a from-scratch Linux-0.11-class kernel in Rust, seeded from the rCore v3 tutorial code; the roadmap and specs live in `docs/spec/`.

The kernel crate lives in `os/`; user programs and the user library live in `user/`.
Two leaf modules live in their own repositories, mounted as submodules: the C frame
allocator at `alloc/` (`docs/alloc.md`) and the filesystem crate at `fs/`
(`docs/fs.md`). A fresh clone needs `git submodule update --init`.

- `os/src/main.rs` is the kernel entry after `entry.asm`.
- `os/src/trap/`, `os/src/task/`, `os/src/syscall/`, `os/src/mm/`, and `os/src/sync/` hold the main kernel subsystems.
- `user/src/lib.rs` provides the user-space support library.

## Build, Test, and Development Commands

- `cd os && make run`: build and boot the kernel in `qemu-system-riscv64`.
- `make verify`: the machine gate — build every target, run the host tests, boot the riscv64 suite, and write `verify/report.json`. `make verify-negative` proves the gate fails on a known-bad boot; `make lint` runs fmt and clippy.

The toolchain in `rust-toolchain.toml` includes `rustfmt`, `clippy`, LLVM tools, and the RISC-V target.

## Coding Style & Naming Conventions

Use Rust 2024 edition and standard `rustfmt` formatting with 4-space indentation. Keep kernel code `#![no_std]` and preserve explicit unsafe boundaries. The kernel denies warnings and missing docs, so new public items need useful `///` docs.

Use Rust naming conventions: modules and functions in `snake_case`, types in `CamelCase`, constants in `SCREAMING_SNAKE_CASE`. Assembly files use `.S` and are included with `global_asm!`.

## Testing Guidelines

Run `make verify`; `cd user && make build` must also pass with no errors.

## Commits

One purpose per commit; a short imperative subject, no long body.
