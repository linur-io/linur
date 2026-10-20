# Repository Guidelines

## Project Structure & Module Organization

`linur` (LINUx + Rust) is a from-scratch Linux-0.11-class kernel in Rust, seeded from the rCore v3 tutorial code; the roadmap and specs live in `docs/spec/`.

The kernel crate lives in `os/`; user programs and the user library live in `user/`.

- `os/src/main.rs` is the kernel entry after `entry.asm`.
- `os/src/trap/`, `os/src/task/`, `os/src/syscall/`, `os/src/mm/`, and `os/src/sync/` hold the main kernel subsystems.
- `user/src/lib.rs` provides the user-space support library.

## Build, Test, and Development Commands

- `cd os && make run`: build and boot the kernel in `qemu-system-riscv64`.

The pinned toolchain in `rust-toolchain.toml` includes `rustfmt`, `clippy`, LLVM tools, and the RISC-V target.

## Coding Style & Naming Conventions

Use Rust 2024 edition and standard `rustfmt` formatting with 4-space indentation. Keep kernel code `#![no_std]` and preserve explicit unsafe boundaries. The kernel denies warnings and missing docs, so new public items need useful `///` docs.

Use Rust naming conventions: modules and functions in `snake_case`, types in `CamelCase`, constants in `SCREAMING_SNAKE_CASE`. Assembly files use `.S` and are included with `global_asm!`.

## Testing Guidelines

just run `cd user && make build` and no error
