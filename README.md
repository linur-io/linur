# linur

**linur = LINUx + Rust.**

linur is a from-scratch reimplementation of a Linux-0.11-class kernel in Rust: 0.11's
subsystem inventory and user-visible experience, implemented behind a modern
Linux-shaped interface — Linux syscall numbers, errno returns, ELF executables, with C
admitted only for small leaf modules. The point is that ordinary test oracles can be
borrowed instead of invented.

## Lineage

linur is seeded from the rCore v3 tutorial code and inherits its GPLv3 license; see
[NOTICE](NOTICE) and [docs/spec/01-rCore-vs-linux011.md](docs/spec/01-rCore-vs-linux011.md)
Appendix A for the recovered lab commits the porting work reads from.

## Roadmap

[docs/spec/02-roadmap.md](docs/spec/02-roadmap.md): six milestones from M1 (foundation)
to M6 (0.11 parity, v1.0).

## Build and run

Prerequisites: the pinned Rust toolchain from
[rust-toolchain.toml](rust-toolchain.toml) with the `x86_64-unknown-none`
target (`rustup target add x86_64-unknown-none`, for the compile-only check and
the smoke path), `clang` (the C frame allocator in `alloc/` is compiled by
`os/build.rs`; an unversioned or versioned `clang` is found automatically),
`qemu-system-riscv64` >= 7, and `qemu-system-x86_64` for the smoke path. A fresh
clone needs `git submodule update --init` for `alloc/`.

```sh
cd os && make run
```

That builds the kernel for `riscv64gc-unknown-none-elf`, packs `fs.img` from a staged
userland and boots it in QEMU. Once the dependencies are cached the build runs offline
(`CARGO_NET_OFFLINE=true`); the tracked `Cargo.lock` files keep it reproducible.

## Tests

```sh
cd os && make run TEST=1
```

`TEST=1` packs `usertests` as init instead of the shell: the kernel runs the userland
suite in QEMU and exits 0 only if every expected success and every expected failure
holds. Packing never writes into the cargo target directory, so a run cannot corrupt
the cache.

`make verify`, from the repository root, is the machine gate: the riscv64 suite
plus the x86_64 smoke path (`x86-smoke/`: own loader → long mode → clean exit) and
the x86_64 compile-only check, with `verify/report.json` as the artifact.

## Debugging

Panic traces, the crash-surviving serial log, QEMU monitor and gdb recipes:
[docs/debugging.md](docs/debugging.md).

## License

GPL-3.0-only. The full text is [LICENSE](LICENSE); attribution is [NOTICE](NOTICE).
