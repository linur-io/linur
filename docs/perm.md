# linur — permissions, mounts and device nodes (M2.3)

Status: interface contract for M2.3. Read with `spec/02` (M2.3), `spec/01` §4/§5,
`docs/vfs.md` (M2.2) and `docs/errors.md`. M2.3 gives the filesystem the metadata
and the checks 0.11 has: mode/uid/gid/umask on disk, the permission predicate,
`chmod`/`chown`/`umask`/`access`/`mknod`, a mount table holding one
filesystem, and a `/dev` skeleton whose nodes are real inodes.

## 1. Scope

In: on-disk `mode`/`uid`/`gid` and the device number of a special file; the
process credentials `uid`/`gid`/`umask`; `Cred` and `permission()`; the checks
at the 0.11 call sites; `chmod`, `chown`, `umask`, `access`, `mknod`; the
mount table with exactly one filesystem; `/dev` and its device nodes in every
image; `stat` reporting the real mode, owner and device.

Out: `setuid`/`setgid`/`setreuid`/`getuid`/`geteuid` and the rest of the
credential calls (M6.1) — until then every process is the superuser, exactly as
0.11's boot is; major/minor dispatch and the device table (M3.1); the real
`/dev/null`, `/dev/port` and the memory devices (M6.2); `mount()`/`umount()` and
`chroot` (M6.1); per-inode superblock back-pointers (M6.1); timestamps (M3.5).

## 2. Credential model

* `ProcessControlBlockInner` gains `uid: u32`, `gid: u32`, `umask: u16`. The
  init process starts as uid 0, gid 0, umask `0o022`; `fork`, `exec` and thread
  creation inherit them unchanged.
* A caller's identity is snapshotted into `os/src/fs/perm.rs`:

```rust
/// The identity a permission check runs as.
#[derive(Clone, Copy)]
pub struct Cred { pub uid: u32, pub gid: u32 }
impl Cred {
    /// The superuser, for boot-time path resolution.
    pub const ROOT: Cred;
    /// The calling process's identity.
    pub fn current() -> Self;
}
```

* `permission(inode, mask, cred)` is the one predicate. It returns `EACCES`:

```rust
pub const MAY_EXEC: u8 = 1;
pub const MAY_WRITE: u8 = 2;
pub const MAY_READ: u8 = 4;
pub fn permission(inode: &Inode, mask: u8, cred: Cred) -> Result<(), isize>;
```

  `cred.uid == 0` grants everything (`suser()` in 0.11). Otherwise the owner
  bits (`mode >> 6`), then the group bits (`mode >> 3`), then the other bits
  decide, and every requested bit must be granted.
* `uid` is the effective identity: `permission` compares `cred.uid` against the
  inode's `uid`. M6.1 splits real/effective/saved identities; M2.3 has one pair.
* A new inode is owned by the creating process: `uid` and `gid` come from
  `cred` (0.11 sets `i_uid = current->euid`, `i_gid = current->egid`).
* `umask(mask)` stores `mask & 0o777` and returns the previous value. It is
  applied to `open` with `O_CREAT`, `mkdir` and `mknod`; it is **not** applied by
  `chmod`. Ownership and umask inheritance are what makes `fork` + `creat` look
  like 0.11's.

## 3. On-disk format v3 (easy-fs)

§2 of `docs/vfs.md` describes v2; v3 is v2 plus ownership and the file type moved
into `mode`. There is no compatibility: every build regenerates `fs.img` and the
host test recreates its own image.

Disk inode, still exactly 128 bytes, four per 512-byte block:

| offset | field | type | meaning |
|---|---|---|---|
| 0 | `size` | `u32` | file size in bytes |
| 4 | `nlink` | `u32` | hard-link count |
| 8 | `mode` | `u16` | `S_IFMT` type plus `0o7777` permission bits |
| 10 | `uid` | `u16` | owner |
| 12 | `gid` | `u16` | group |
| 14 | `_reserved` | `u16` | zero |
| 16 | `direct` | `[u32; 26]` | direct data blocks, or `direct[0]` = the device number |
| 120 | `indirect1` | `u32` | single indirect block |
| 124 | `indirect2` | `u32` | double indirect block |

* The type lives in `mode`, so `DiskInodeType` disappears and `is_dir`/`is_file`
  read `mode & S_IFMT`. `S_IFDIR` 0o040000, `S_IFREG` 0o100000, `S_IFCHR`
  0o020000, `S_IFBLK` 0o060000, `S_IFIFO` 0o010000, `S_IFMT` 0o170000, all
  exported by `easy-fs` as `u16`.
* Direct entries drop 27 → 26: the 4-byte `DiskInodeType` becomes the 8-byte
  metadata header (`mode`, `uid`, `gid`, `_reserved`), so one 4-byte direct entry
  pays for the difference. The maximum file size becomes
  (26 + 128 + 16384) × 512 B ≈ 8.1 MiB; the big-file host test crosses every
  boundary, so it must stay green.
* `uid` and `gid` are 16-bit on disk and widened to `u32` by `stat`.
* **Device number.** For `S_IFCHR` and `S_IFBLK` inodes, `direct[0]` holds the
  device number `dev = (major << 8) | minor` and the inode never allocates a data
  block. `create_node` is the only creator that sets it; a FIFO node keeps no
  meaningful device number and `dev()` reports 0 for it. `dev()` also returns 0
  for every other type, so `direct[0]` of a regular file (its first data block)
  is never misread as one.

## 4. easy-fs API

`easy-fs/src/vfs.rs` and `easy-fs/src/layout.rs`:

```rust
impl Inode {
    pub fn mode(&self) -> u16;
    pub fn uid(&self) -> u16;
    pub fn gid(&self) -> u16;
    pub fn dev(&self) -> u32;
    pub fn set_mode(&self, mode: u16);
    pub fn set_owner(&self, uid: u16, gid: u16);
    /// Create a regular file; `mode` is the permission bits only.
    pub fn create_file(&self, name: &str, mode: u16, uid: u16, gid: u16) -> Result<Arc<Inode>, VfsError>;
    /// Create a directory; `mode` is the permission bits only.
    pub fn create_dir(&self, name: &str, mode: u16, uid: u16, gid: u16) -> Result<Arc<Inode>, VfsError>;
    /// Create a special file; `mode` carries the type (`S_IFCHR`, `S_IFBLK`, ...).
    pub fn create_node(&self, name: &str, mode: u16, uid: u16, gid: u16, dev: u32) -> Result<Arc<Inode>, VfsError>;
}
```

Rules:

* `create_file` adds `S_IFREG`, `create_dir` adds `S_IFDIR`; `create_node` stores
  `mode` as given and the kernel validates its type before calling. All three
  keep the v2 name checks, `Exists`/`NotDirectory` refusals, `nlink` rules and
  `.`/`..` writing for directories.
* `set_mode` writes the whole word; the kernel passes
  `(old_mode & S_IFMT) | perms` so the type survives `chmod`.
* `create` for the root inode (`EasyFileSystem::create`) writes
  `S_IFDIR | 0o755`, uid 0, gid 0.
* The packing tool creates every app as `S_IFREG` with `0o755` and then the
  `/dev` skeleton of §9 from the same API.

## 5. Kernel objects and API

`os/src/fs/vfs.rs`: `Inode` speaks the user-visible `u32` mode, so `stat` and the
syscalls never cast at the boundary:

```rust
impl Inode {
    pub fn mode(&self) -> u32;
    pub fn uid(&self) -> u32;
    pub fn gid(&self) -> u32;
    pub fn dev(&self) -> u32;
    pub fn set_mode(&self, mode: u32);
    pub fn set_owner(&self, uid: u32, gid: u32);
    /// Create a regular file; `perms` are permission bits, the type is added here.
    pub fn create_file(&self, name: &str, perms: u32, cred: Cred) -> Result<Arc<Inode>, isize>;
    /// Create a directory; `perms` are permission bits, the type is added here.
    pub fn create_dir(&self, name: &str, perms: u32, cred: Cred) -> Result<Arc<Inode>, isize>;
    /// Create a special file; `mode` carries the type.
    pub fn create_node(&self, name: &str, mode: u32, cred: Cred, dev: u32) -> Result<Arc<Inode>, isize>;
}
```

The path functions take the caller's identity, because the walker checks search
permission and boot-time resolution has no process to read it from:

```rust
pub fn lookup(cwd: &Arc<Dentry>, path: &str, cred: Cred) -> Result<Arc<Dentry>, isize>;
pub fn lookup_parent(cwd: &Arc<Dentry>, path: &str, cred: Cred) -> Result<(Arc<Dentry>, String), isize>;
pub fn open(cwd: &Arc<Dentry>, path: &str, flags: OpenFlags, mode: u32, cred: Cred) -> Result<Arc<OSInode>, isize>;
```

`open`'s `mode` is the permission bits a `O_CREAT` file gets, already masked by
umask. A special file (`S_IFCHR`, `S_IFBLK`, `S_IFIFO`) opens with `ENXIO`: there
is no device table until M3.1 and no pipe-file until M6.2, and pretending
otherwise would be the dishonesty this milestone avoids. The permission check
runs before that type dispatch, so a caller that may not read a device node gets
`EACCES`, not `ENXIO` (nothing observes the difference while every process is
root). `O_TRUNC` and `O_DIRECTORY` keep their M2.2 meaning for regular files and
directories.

`os/src/fs/mount.rs` (new) owns the mount table:

```rust
/// One mounted filesystem.
struct Mount { dev: u64, root: Arc<Dentry> }
/// The mounted filesystems, in mount order.
pub struct MountTable { mounts: Vec<Mount> }
impl MountTable {
    /// Mount the block device's filesystem at "/".
    fn new() -> Self;
    /// The root dentry of the first mount.
    pub fn root(&self) -> Arc<Dentry>;
    /// The device number every inode of M2.3 belongs to.
    pub fn dev(&self) -> u64;
}
pub fn root() -> Arc<Dentry>;
pub fn dev() -> u64;
```

`fs::root()` and `stat()`'s `st_dev` move here; `vfs::root()` and the
`SUPER_BLOCK` lazy-static disappear. M2.3 has one mount, so `dev()` is its device
number and the walker never crosses a mount point; M6.1's `mount()` adds entries
and gives `Inode` a superblock back-pointer so `stat` picks the right device.

## 6. Where the checks run

| operation | check |
|---|---|
| `lookup`/`lookup_parent`: each directory descended into | `MAY_EXEC` on it |
| `open` of an existing regular file | `MAY_READ` and/or `MAY_WRITE` per access mode |
| `open` of an existing directory | `MAY_READ` (it is opened read-only for `getdents`) |
| `open` with `O_CREAT` on a missing name | `MAY_WRITE|MAY_EXEC` on the parent |
| `mkdir` | `MAY_WRITE|MAY_EXEC` on the parent |
| `mknod` | the same, plus `EPERM` unless root (0.11 checks `suser()`) |
| `unlink`/`rmdir` | `MAY_WRITE|MAY_EXEC` on the parent |
| `link` | `MAY_WRITE|MAY_EXEC` on the new parent |
| `rename` | `MAY_WRITE|MAY_EXEC` on both parents |
| `chdir` | `MAY_EXEC` on the target |
| `exec`, `spawn` | a regular file, then `MAY_EXEC` on it; otherwise `EACCES` |
| `access` | exactly the requested bits; `F_OK` checks existence only |
| `chmod` | `EPERM` unless root or the owner |
| `chown` | `EPERM` unless root |
| `stat`, `fstat`, `getcwd`, `close`, `read`/`write` on an open file | none |

**Honesty note.** In M2.3 every process is the superuser, so every row above is
granted and only two refusals are observable: `exec` of a non-regular file is
`EACCES` (independent of credentials), and the `EPERM` rows are unreachable.
That is the same user-visible behaviour as 0.11, whose processes are all root and
whose `suser()` is always true; the deny path becomes observable when M6.1 adds
the credential calls. The suite therefore proves the metadata (mode, owner,
umask, rdev, device) and the root bypass, not the denial.

## 7. Syscall table and user ABI

New entries; numbers follow the M2.2 convention of the `*at` number with the
leading `dirfd` implicit (cwd-relative), as `mkdir` 34 / `unlink` 35 already do:

| number | call | arguments | success | errors |
|---|---|---|---|---|
| 33 | `mknod` | `path`, `mode`, `dev` | 0 | `EEXIST`, `ENOENT`, `ENOTDIR`, `EINVAL`, `EPERM`, `EACCES`, `ENAMETOOLONG`, `EFAULT` |
| 48 | `access` | `path`, `mode` | 0 | `EACCES`, `EINVAL`, `ENAMETOOLONG`, `EFAULT` |
| 53 | `chmod` | `path`, `mode` | 0 | `ENOENT`, `ENOTDIR`, `EPERM`, `ENAMETOOLONG`, `EFAULT` |
| 54 | `chown` | `path`, `uid`, `gid` | 0 | `ENOENT`, `ENOTDIR`, `EPERM`, `ENAMETOOLONG`, `EFAULT` |
| 166 | `umask` | `mask` | the previous mask | — |

* `mknod` accepts `S_IFREG`, `S_IFCHR`, `S_IFBLK` and `S_IFIFO`; any other type,
  including `S_IFDIR` and a bare permission word, is `EINVAL`. The permission
  bits are masked by umask and owned by the caller.
* `chmod` takes the permission bits only (`mode & 0o7777`); the type is kept.
* `chown` follows the Linux convention: `uid` or `gid` equal to `u32::MAX`
  leaves that half unchanged. 0.11 assigns the argument literally, so this one
  semantic difference is recorded in `spec/01` §4.
* `access` requires `mode` to be within 0o7, else `EINVAL`. A path that does not
  resolve is `EACCES`, not `ENOENT`: 0.11's `sys_access` returns `-EACCES` when
  `namei` fails, and the checklist asks for the same behaviour.
* `open`'s `mode` and `mkdir`'s `mode` are now used (§6 of `docs/vfs.md` said
  "accepted and ignored until M2.3").
* `exec` of a directory now returns `EACCES`, not `ENOEXEC` (the refinement
  `docs/vfs.md` §6 promised to this milestone).

`user/src/lib.rs` gains:

```rust
pub fn mknod(path: &str, mode: u32, dev: u32) -> isize;
pub fn chmod(path: &str, mode: u32) -> isize;
pub fn chown(path: &str, uid: u32, gid: u32) -> isize;
pub fn umask(mask: u32) -> u32;
pub fn access(path: &str, mode: u32) -> isize;
pub const F_OK: u32 = 0;
pub const W_OK: u32 = 2;
pub const R_OK: u32 = 4;
pub const X_OK: u32 = 1;
pub const S_IFBLK: u32 = 0o060000;
/// `(major << 8) | minor`, the encoding of every device number.
pub fn makedev(major: u32, minor: u32) -> u32;
```

## 8. `struct stat`

Fields that change value in M2.3 (`docs/vfs.md` §8 keeps the layout):

| field | value |
|---|---|
| `st_dev` | 1 — the one mount's device number |
| `st_mode` | the inode's on-disk `mode` widened to `u32` |
| `st_uid`, `st_gid` | the inode's owner |
| `st_rdev` | the device number of a `S_IFCHR`/`S_IFBLK` inode, else 0 |
| `st_nlink` | unchanged |
| `st_size`, `st_blocks` | 0 for a device node |

`Stdin`/`Stdout` keep `S_IFCHR | 0o666` with `st_rdev` 0 (the console is not a
device node yet; M3.1 wires it), pipes keep `S_IFIFO | 0o600`.

## 9. `/dev` skeleton

The image packer creates, in every image:

* `/dev`, a directory, mode `0o755`, uid 0, gid 0;
* `/dev/null`, `S_IFCHR | 0o666`, device `makedev(1, 3)`;
* `/dev/zero`, `S_IFCHR | 0o666`, device `makedev(1, 5)`;
* `/dev/console`, `S_IFCHR | 0o666`, device `makedev(5, 1)`.

They are ordinary inodes of the one filesystem: `getdents` lists them, `stat`
reports `S_IFCHR` and `rdev`, `unlink` removes them, and `open` answers `ENXIO`
until the device table exists (M3.1) and the devices are real (M6.2). A test may
create further nodes with `mknod` in its own directory.

## 10. Errors

New values, kernel and user lists must match: `EPERM` −1, `ENXIO` −6,
`EACCES` −13. `VfsError` keeps its M2.2 mapping; `EACCES`, `EPERM` and
`ENXIO` are decided in the kernel.

## 11. Seed regression suite

Three programs join `SUCC_TESTS` and the gate count grows 72 → 75
(`scripts/verify.sh`). Each prints `NAME: n/n passed` and exits 0 only when every
case held; a failure prints the case name.

* `perm_test`: the boot umask is `0o022`; `creat(..., 0o666)` gives
  `S_IFREG|0o644`; `umask(0o077)` then `creat(..., 0o666)` gives `0o600` and
  `mkdir(..., 0o777)` gives `S_IFDIR|0o700`; `chmod` rewrites the permission bits
  and keeps the type even when the argument carries type bits; `chown` sets
  uid/gid and `u32::MAX` leaves a half unchanged; `access` returns 0 for the
  owner on every requested bit, `EACCES` on a missing path and `EINVAL` on a
  mode outside 0o7; a root-owned file with mode 0 still opens for reading and
  passes `access(R_OK)` (the root bypass, recorded as behaviour); `chmod`/
  `chown` of a missing path are `ENOENT`; `exec("/")` is `EACCES`.
* `dev_test`: `/dev` is a directory; `/dev/null`, `/dev/zero` and
  `/dev/console` are `S_IFCHR`, mode `0o666`, with the device numbers of §9;
  `getdents` of `/dev` lists exactly the three nodes plus `.` and `..`; opening
  one is `ENXIO` for both read and write; `mknod` creates a character node with
  the requested `rdev` and `nlink` 1, umask masks its permission bits, an existing
  name is `EEXIST`, a missing parent is `ENOENT`, `S_IFDIR` is `EINVAL`, a FIFO
  node stats as `S_IFIFO` and opens as `ENXIO`.
* `mount_test`: `stat("/")`, `stat("/dev")` and every file the test creates report
  `st_dev` 1 (one filesystem); `/` is a directory whose `.` and `..` entries
  both name the root inode; `/dev` is not a separate mount (same `st_dev`, and its
  `..` names `/`); a path under a missing directory is `ENOENT`.

The 72 existing tests stay green. The image format bump means the packed apps are
`0o755` and `/dev` exists in the boot images; no existing assertion changes.

## 12. Deliberately not in M2.3

Credential syscalls (`getuid`/`setuid`/...), the device dispatch table, real
devices, `mount()`/`umount()`, `chroot`, sticky-bit and setgid-directory rules
(0.11 has neither), ACLs, and symlink-aware permission checks (there are no
symlinks).
