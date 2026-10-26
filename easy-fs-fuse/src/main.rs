// SPDX-License-Identifier: GPL-3.0-only
use clap::{App, Arg};
use easy_fs::{makedev, BlockDevice, EasyFileSystem, S_IFCHR};
#[cfg(test)]
use easy_fs::{Inode, VfsError, S_IFDIR, S_IFMT, S_IFREG};
use std::fs::{read_dir, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

const BLOCK_SZ: usize = 512;

struct BlockFile(Mutex<File>);

impl BlockDevice for BlockFile {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        let mut file = self.0.lock().unwrap();
        file.seek(SeekFrom::Start((block_id * BLOCK_SZ) as u64))
            .expect("Error when seeking!");
        assert_eq!(file.read(buf).unwrap(), BLOCK_SZ, "Not a complete block!");
    }

    fn write_block(&self, block_id: usize, buf: &[u8]) {
        let mut file = self.0.lock().unwrap();
        file.seek(SeekFrom::Start((block_id * BLOCK_SZ) as u64))
            .expect("Error when seeking!");
        assert_eq!(file.write(buf).unwrap(), BLOCK_SZ, "Not a complete block!");
    }
}

fn main() {
    easy_fs_pack().expect("Error when packing easy-fs!");
}

/// Pack a staged userland directory into `fs.img`.
///
/// Stage contract:
/// * `-s` is a directory of final app ELFs, one regular file per app. Each file
///   is packed under its own name, byte for byte: no renaming, no extension
///   surgery, and an unreadable file is an error rather than a silent skip.
/// * `-t` is a different directory, which receives `fs.img`. It must not be the
///   stage directory, or the image could end up packed into itself.
///
/// Why staging: cargo hardlinks `target/<triple>/release/<bin>` to
/// `target/<triple>/release/deps/<bin>-<hash>`, so copying one built app over
/// another name inside `target/.../release` writes through the hardlink into
/// cargo's cache, and every later build restores the corrupted binary. The
/// stage is a fresh directory of plain copies, so `TEST=1` can replace
/// `initproc` there without touching `target/`.
fn easy_fs_pack() -> std::io::Result<()> {
    let matches = App::new("EasyFileSystem packer")
        .arg(
            Arg::with_name("source")
                .short("s")
                .long("source")
                .takes_value(true)
                .help("Staged app dir: every regular file is packed as-is"),
        )
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .takes_value(true)
                .help("Output dir that receives fs.img"),
        )
        .get_matches();
    let src_path = Path::new(matches.value_of("source").unwrap());
    let target_path = Path::new(matches.value_of("target").unwrap());
    println!(
        "src_path = {}\ntarget_path = {}",
        src_path.display(),
        target_path.display()
    );
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(target_path.join("fs.img"))?;
        f.set_len(16 * 2048 * 512).unwrap();
        f
    })));
    // 16MiB, at most 4095 files
    let efs = EasyFileSystem::create(block_file, 16 * 2048, 1);
    let root_inode = Arc::new(EasyFileSystem::root_inode(&efs));
    for entry in read_dir(src_path)? {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .expect("App name in the stage dir must be valid UTF-8");
        // load app data from the staged copy, never from a cargo target dir
        let mut all_data: Vec<u8> = Vec::new();
        File::open(entry.path())?.read_to_end(&mut all_data)?;
        // create a file in easy-fs
        let inode = root_inode.create_file(name.as_str(), 0o755, 0, 0).unwrap();
        // write data to easy-fs
        inode.write_at(0, all_data.as_slice());
    }
    // the /dev skeleton of docs/perm.md §9
    let dev = root_inode.create_dir("dev", 0o755, 0, 0).unwrap();
    dev.create_node("null", S_IFCHR | 0o666, 0, 0, makedev(1, 3))
        .unwrap();
    dev.create_node("zero", S_IFCHR | 0o666, 0, 0, makedev(1, 5))
        .unwrap();
    dev.create_node("console", S_IFCHR | 0o666, 0, 0, makedev(5, 1))
        .unwrap();
    Ok(())
}

/// Iterate a directory into `(name, inode_id, is_dir)` triples.
#[cfg(test)]
fn dirents(inode: &Inode) -> Vec<(String, u32, bool)> {
    let mut entries = Vec::new();
    let mut offset = 0;
    while let Some(dirent) = inode.dirent_at(offset) {
        offset = dirent.next_offset;
        entries.push((dirent.name, dirent.inode_id, dirent.is_dir));
    }
    entries
}

/// One expected `dirents` triple.
#[cfg(test)]
fn entry(name: &str, inode_id: u32, is_dir: bool) -> (String, u32, bool) {
    (name.to_string(), inode_id, is_dir)
}

/// The error of a failed operation whose success value has no `Debug`.
#[cfg(test)]
fn err<T>(result: Result<T, VfsError>) -> VfsError {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error,
    }
}

#[test]
fn efs_test() -> std::io::Result<()> {
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open("target/fs.img")?;
        f.set_len((4096 * BLOCK_SZ) as u64).unwrap();
        f
    })));
    EasyFileSystem::create(block_file.clone(), 4096, 1);
    let efs = EasyFileSystem::open(block_file.clone());
    let root_inode = Arc::new(EasyFileSystem::root_inode(&efs));

    // The root is a directory owned by root, mode 0o755.
    assert_eq!(root_inode.mode(), S_IFDIR | 0o755);
    assert_eq!(root_inode.uid(), 0);
    assert_eq!(root_inode.gid(), 0);

    // "." and ".." are real entries naming the root itself.
    assert_eq!(root_inode.nlink(), 2);
    assert_eq!(
        dirents(&root_inode),
        vec![entry(".", 0, true), entry("..", 0, true)]
    );
    assert_eq!(root_inode.find(".").unwrap().inode_id(), 0);
    assert_eq!(root_inode.find("..").unwrap().inode_id(), 0);

    // A regular file starts with one link; names are validated.
    let greet = "Hello, world!";
    let filea = root_inode.create_file("filea", 0o644, 0, 0).unwrap();
    assert!(filea.is_file());
    assert_eq!(filea.nlink(), 1);
    assert_eq!(
        err(root_inode.create_file("filea", 0o644, 0, 0)),
        VfsError::Exists
    );
    assert_eq!(
        err(root_inode.create_file("", 0o644, 0, 0)),
        VfsError::InvalidName
    );
    assert_eq!(
        err(root_inode.create_file(&"x".repeat(28), 0o644, 0, 0)),
        VfsError::InvalidName
    );
    assert_eq!(
        err(filea.create_file("child", 0o644, 0, 0)),
        VfsError::NotDirectory
    );

    // Data round-trips and truncate frees it.
    filea.write_at(0, greet.as_bytes());
    assert_eq!(filea.size(), greet.len() as u32);
    let mut buf = [0u8; 32];
    assert_eq!(filea.read_at(0, &mut buf), greet.len());
    assert_eq!(core::str::from_utf8(&buf[..greet.len()]).unwrap(), greet);
    filea.truncate();
    assert_eq!(filea.size(), 0);
    assert_eq!(filea.read_at(0, &mut buf), 0);
    filea.write_at(0, greet.as_bytes());
    assert_eq!(filea.read_at(0, &mut buf), greet.len());
    assert_eq!(filea.read_at(1024, &mut buf), 0);

    // A new directory names itself and its parent and bumps the parent's nlink.
    let d1 = root_inode.create_dir("d1", 0o755, 0, 0).unwrap();
    assert!(d1.is_dir());
    assert_eq!(d1.nlink(), 2);
    assert_eq!(root_inode.nlink(), 3);
    assert_eq!(
        dirents(&d1),
        vec![entry(".", d1.inode_id(), true), entry("..", 0, true)]
    );
    assert_eq!(
        err(root_inode.create_dir("d1", 0o755, 0, 0)),
        VfsError::Exists
    );

    // Hard links share the inode and its data.
    root_inode.link("filea2", &filea).unwrap();
    assert_eq!(filea.nlink(), 2);
    let filea2 = root_inode.find("filea2").unwrap();
    assert_eq!(filea2.inode_id(), filea.inode_id());
    assert_eq!(filea2.read_at(0, &mut buf), greet.len());
    assert_eq!(
        root_inode.link("filea2", &filea).unwrap_err(),
        VfsError::Exists
    );
    assert_eq!(
        root_inode.link("d1link", &d1).unwrap_err(),
        VfsError::IsDirectory
    );
    root_inode.remove("filea2").unwrap();
    assert_eq!(filea.nlink(), 1);
    assert!(root_inode.find("filea2").is_none());
    assert_eq!(root_inode.remove("filea2").unwrap_err(), VfsError::NotFound);
    assert_eq!(root_inode.remove(".").unwrap_err(), VfsError::InvalidName);

    // Rename within a directory, replacing an existing regular file.
    root_inode
        .rename("filea", &root_inode, "filea_renamed")
        .unwrap();
    assert!(root_inode.find("filea").is_none());
    let filea_renamed = root_inode.find("filea_renamed").unwrap();
    assert_eq!(filea_renamed.inode_id(), filea.inode_id());
    root_inode
        .create_file("fileb", 0o644, 0, 0)
        .unwrap()
        .write_at(0, b"old");
    root_inode
        .rename("filea_renamed", &root_inode, "fileb")
        .unwrap();
    let fileb = root_inode.find("fileb").unwrap();
    assert_eq!(fileb.inode_id(), filea.inode_id());
    assert_eq!(fileb.read_at(0, &mut buf), greet.len());

    // Rename across directories: the data follows the inode.
    let d2 = root_inode.create_dir("d2", 0o755, 0, 0).unwrap();
    assert_eq!(root_inode.nlink(), 4);
    let moved = root_inode.create_file("moved", 0o644, 0, 0).unwrap();
    moved.write_at(0, b"data");
    root_inode.rename("moved", &d2, "moved2").unwrap();
    assert!(root_inode.find("moved").is_none());
    let moved2 = d2.find("moved2").unwrap();
    assert_eq!(moved2.inode_id(), moved.inode_id());
    assert_eq!(moved2.read_at(0, &mut buf), 4);

    // Moving a directory rewrites its ".." entry and both parents' nlink.
    root_inode.rename("d1", &d2, "d1_moved").unwrap();
    assert_eq!(root_inode.nlink(), 3);
    assert_eq!(d2.nlink(), 3);
    assert_eq!(d1.find("..").unwrap().inode_id(), d2.inode_id());
    assert_eq!(d2.find("d1_moved").unwrap().inode_id(), d1.inode_id());

    // A directory cannot be moved into its own subtree.
    assert_eq!(
        d2.rename("d1_moved", &d1, "inside").unwrap_err(),
        VfsError::InvalidName
    );
    assert_eq!(
        root_inode.rename("d2", &d1, "d2_in_d1").unwrap_err(),
        VfsError::InvalidName
    );

    // rmdir refuses a non-empty directory and frees directories bottom-up.
    assert_eq!(root_inode.remove("d2").unwrap_err(), VfsError::NotEmpty);
    d2.remove("moved2").unwrap();
    d2.remove("d1_moved").unwrap();
    assert_eq!(d2.nlink(), 2);
    root_inode.remove("d2").unwrap();
    assert_eq!(root_inode.nlink(), 2);
    assert!(root_inode.find("d2").is_none());

    // A directory spanning several data blocks iterates and shrinks correctly.
    let many = root_inode.create_dir("many", 0o755, 0, 0).unwrap();
    const MANY: usize = 40;
    for i in 0..MANY {
        many.create_file(&format!("f{}", i), 0o644, 0, 0).unwrap();
    }
    let entries = dirents(&many);
    assert_eq!(entries.len(), MANY + 2);
    assert_eq!(entries[0], entry(".", many.inode_id(), true));
    assert_eq!(entries[1], entry("..", 0, true));
    assert_eq!(
        entries[2],
        entry("f0", many.find("f0").unwrap().inode_id(), false)
    );
    assert_eq!(entries[MANY + 1].0, format!("f{}", MANY - 1));
    for i in 0..MANY {
        many.remove(&format!("f{}", i)).unwrap();
    }
    assert_eq!(
        dirents(&many),
        vec![entry(".", many.inode_id(), true), entry("..", 0, true)]
    );
    root_inode.remove("many").unwrap();
    assert_eq!(root_inode.nlink(), 2);

    // Big files cross the direct, indirect1 and indirect2 boundaries.
    let big = fileb.clone();
    let random_str_test = |len: usize| {
        big.truncate();
        assert_eq!(big.size(), 0);
        let mut data = String::new();
        for _ in 0..len {
            data.push(char::from(b'0' + rand::random::<u8>() % 10));
        }
        big.write_at(0, data.as_bytes());
        assert_eq!(big.size(), len as u32);
        let mut read_buffer = [0u8; 127];
        let mut offset = 0usize;
        let mut read_data = String::new();
        loop {
            let n = big.read_at(offset, &mut read_buffer);
            if n == 0 {
                break;
            }
            offset += n;
            read_data.push_str(core::str::from_utf8(&read_buffer[..n]).unwrap());
        }
        assert_eq!(data, read_data);
    };
    random_str_test(4 * BLOCK_SZ);
    random_str_test(8 * BLOCK_SZ + BLOCK_SZ / 2);
    random_str_test(26 * BLOCK_SZ);
    random_str_test(27 * BLOCK_SZ);
    random_str_test(27 * BLOCK_SZ + 1);
    random_str_test(100 * BLOCK_SZ);
    random_str_test(154 * BLOCK_SZ);
    random_str_test(155 * BLOCK_SZ);
    random_str_test(155 * BLOCK_SZ + 1);
    random_str_test(400 * BLOCK_SZ);
    random_str_test(1000 * BLOCK_SZ);
    random_str_test(2000 * BLOCK_SZ);

    assert_eq!(root_inode.nlink(), 2);

    // Metadata round-trips: mode, owner and device number of every inode kind.
    let owned = root_inode.create_file("owned", 0o640, 7, 8).unwrap();
    assert_eq!(owned.mode(), S_IFREG | 0o640);
    assert_eq!((owned.uid(), owned.gid()), (7, 8));
    assert_eq!(owned.dev(), 0);
    let meta = root_inode.create_dir("meta", 0o750, 9, 10).unwrap();
    assert_eq!(meta.mode(), S_IFDIR | 0o750);
    assert_eq!((meta.uid(), meta.gid()), (9, 10));
    let null = root_inode
        .create_node("null", S_IFCHR | 0o666, 11, 12, makedev(1, 3))
        .unwrap();
    assert_eq!(null.mode(), S_IFCHR | 0o666);
    assert_eq!((null.uid(), null.gid()), (11, 12));
    assert_eq!(null.dev(), makedev(1, 3));
    assert_eq!(null.nlink(), 1);
    assert_eq!(null.size(), 0);
    assert!(!null.is_file());
    assert!(!null.is_dir());
    // set_mode keeps the type and set_owner rewrites the owner.
    owned.set_mode((owned.mode() & S_IFMT) | 0o600);
    owned.set_owner(1, 2);
    // A regular file's direct[0] is a data block, never read as a device number.
    owned.write_at(0, b"data");
    assert_eq!(owned.dev(), 0);
    owned.truncate();
    assert_eq!(owned.dev(), 0);

    // Reopen the image: the metadata above survives.
    let efs = EasyFileSystem::open(block_file.clone());
    let root_inode = Arc::new(EasyFileSystem::root_inode(&efs));
    assert_eq!(root_inode.mode(), S_IFDIR | 0o755);
    let owned = root_inode.find("owned").unwrap();
    assert_eq!(owned.mode(), S_IFREG | 0o600);
    assert_eq!((owned.uid(), owned.gid()), (1, 2));
    assert_eq!(owned.dev(), 0);
    let meta = root_inode.find("meta").unwrap();
    assert_eq!(meta.mode(), S_IFDIR | 0o750);
    assert_eq!((meta.uid(), meta.gid()), (9, 10));
    let null = root_inode.find("null").unwrap();
    assert_eq!(null.mode(), S_IFCHR | 0o666);
    assert_eq!((null.uid(), null.gid()), (11, 12));
    assert_eq!(null.dev(), makedev(1, 3));
    assert_eq!(null.nlink(), 1);
    assert_eq!(null.size(), 0);

    Ok(())
}
