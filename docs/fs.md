# linur — the filesystem module and its repository (M2.7)

Status: interface contract for M2.7. Read with `spec/02` (M2.7), `spec/01` §3,
`docs/vfs.md` §2–§3 (the on-disk format and the crate's API), `docs/block.md` §6–§7
(the cache and its lock), and `docs/alloc.md` §6 (the submodule pattern this step
reuses). M2.7 is structural: the kernel's filesystem crate moves out to
`linur-io/fs`, is consumed as a submodule at `fs/`, and takes its host tests with it.

## 1. Scope

In: the repository split (`easy-fs` → `linur-io/fs`, mounted at `fs/`); the std
adapter crate that carries the host tests; the kernel's path dependencies and gate
wiring; the removal of the crate directory from the kernel tree.

Out: any change to the on-disk format, the VFS, the cache or the driver (M2.2–M2.5
own those; this step moves files and changes no behaviour). The packer
(`easy-fs-fuse`) stays in the kernel tree: it is build tooling for `fs.img`, not the
filesystem. The `fs` repository is not pushed in this step (see §6).

## 2. What moves, what stays

| today | after M2.7 |
|---|---|
| `easy-fs/` (no_std crate, 8 source files, 1,600 lines) | `fs/` submodule of `linur-io/fs`, package name unchanged (`easy-fs`), so `use easy_fs::...` in the kernel does not change |
| `easy-fs-fuse/src/main.rs`: packer **and** the single host test | packer stays in the kernel tree; the test moves to `fs/host/tests/efs.rs` |
| `easy-fs-fuse`'s private file-backed `BlockDevice` | `easy-fs-host::BlockFile`, shared by the test and the packer |
| `cargo test` inside `easy-fs-fuse` as the gate's filesystem host test | `cargo test --workspace` inside `fs/`, run by the same gate phase |
| `rand` in `easy-fs-fuse` (test-only) | gone: the moved test generates its digits from a three-line xorshift |

The repository keeps its own CI (fmt, clippy, workspace tests, and a riscv64
freestanding check) because the crate is compiled for `riscv64gc-unknown-none-elf` by
the kernel and for the host by its tests.

## 3. The repository

```
fs/                     linur-io/fs, mounted at the kernel's root as `fs/`
  Cargo.toml            package easy-fs (the crate is the repository root)
  Cargo.lock
  src/*.rs              moved verbatim; edition 2024
  host/
    Cargo.toml          package easy-fs-host, std, only dependency: easy-fs = { path = ".." }
    src/lib.rs          BlockFile: a 512-byte-block file device with read/write counters
    tests/efs.rs        the host test of §7
  README.md NOTICE LICENSE .gitignore .github/workflows/ci.yml
```

The crate is the repository root on purpose: the kernel then depends on `../fs` for
the filesystem and on `../fs/host` for host tooling, and the submodule path is the
crate. `Cargo.toml` makes `host` a workspace member, so one `Cargo.lock` covers both.
The package name stays `easy-fs`: the kernel's `use easy_fs::` lines and the
`deny.toml` allow-list entry are correct before and after, and a rename would be a
different step. Edition 2024 matches the rest of the tree (the source needs no
change for it: no `unsafe fn`, no `static mut`, no `extern "C"`).

## 4. The std adapter

`easy-fs-host` exists because the crate is `no_std` and allocation-only: it cannot
open a file, so every host test and the packer need a std `BlockDevice`. The adapter
is deliberately tiny — one type and three methods:

```rust
pub struct BlockFile { /* Mutex<File>, 512-byte blocks, read/write counters */ }

impl BlockFile {
    /// Create or truncate `path` as an image of `blocks` blocks.
    pub fn create(path: impl AsRef<Path>, blocks: usize) -> io::Result<Self>;
    /// Device reads so far.
    pub fn reads(&self) -> usize;
    /// Device writes so far.
    pub fn writes(&self) -> usize;
}

impl easy_fs::BlockDevice for BlockFile { /* seek + one exact block, counted */ }
```

The counters are not decoration: the cache tests assert that a delayed write touches
no device and that a dirty block is served from the cache, which is only observable
through the device. They live in the adapter because they are host-test concerns, not
filesystem state.

## 5. The kernel side

- `os/Cargo.toml`: `easy-fs = { path = "../fs" }`.
- `easy-fs-fuse/Cargo.toml`: `easy-fs = { path = "../fs" }` and
  `easy-fs-host = { path = "../fs/host" }`; `rand` leaves (the packer never used it —
  only the test did).
- `easy-fs-fuse/src/main.rs` loses the device, the test and the test-only imports;
  the packer keeps its argument handling, the stage contract and the `/dev` skeleton.
- Root `Makefile` lint and `.github/workflows/ci.yml` point at `fs/` instead of
  `easy-fs/`; the cache lists `fs/target` and `fs/host/target` (the generic
  `**/target/` ignore already covers both).
- `scripts/verify.sh`: the host-test phase runs `cargo test --workspace` in `fs/`;
  the packer still has to build because `make fs-img` runs it.

Nothing in `os/src` changes.

## 6. The pin, and the push the user withheld

`fs/` is a submodule of `linur-io/fs` pinned to `v0.1.0`; `.gitmodules` carries the
HTTPS URL for the same reason `alloc/` does (CI checks out submodules without an SSH
key). This step exercises the pin workflow a second time, but with a twist: the
repository has not been pushed, so the gitlink names a commit that exists only in the
maintainer's clone. Until it is pushed:

- the kernel tree needs no change before or after the push (the gitlink and
  `.gitmodules` already name the final commit and URL);
- a fresh clone with `--recurse-submodules`, and therefore CI, fails to fetch `fs/`;
- a local clone can be made to work by rewriting the URL for that clone only
  (`git -c url."<path-to-repo>".insteadOf=https://github.com/linur-io/fs.git clone ...`).

The push sequence, when the maintainer allows it:

```sh
cd <linur-io/fs clone> && git push origin main v0.1.0
# the submodule was cloned through a local rewrite, so teach it the real URL:
git -C fs remote set-url origin https://github.com/linur-io/fs.git   # or: git submodule sync
```

No kernel commit is needed to complete the pin.

## 7. Tests

The moved test is the same single `#[test] fn efs_test` that ran inside
`easy-fs-fuse`: names and `.`/`..`, metadata round-trips (mode, owner, device
number, nlink), hard links and their removal, rename within and across directories
(including the `..` rewrite when a directory moves), rmdir refusing a non-empty
directory, a directory that spans several data blocks and shrinks back, files across
the direct/indirect1/indirect2 boundaries, a reopen of the image, the delayed-write
and cache-hit device counts, the 150-block capacity run past the 64-block cache, and
data-block reclamation up to the indirect2 boundary.

It stays one test on purpose: the block cache is a process-wide singleton
(`BLOCK_CACHE_MANAGER`), so two tests in parallel would share and evict each other's
blocks. A single serial test is what makes the device-count assertions meaningful —
the same constraint `notes/m23.md` pitfall 4 recorded when the cache was rewritten.

The gate phase is unchanged in meaning: host tests green means the moved test green.
`make verify` still builds the packer and boots the 77-test suite on the new image.
