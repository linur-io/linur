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
[rust-toolchain.toml](rust-toolchain.toml) and `qemu-system-riscv64` >= 7.

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

## License

GPL-3.0-only. The full text is [LICENSE](LICENSE); attribution is [NOTICE](NOTICE).