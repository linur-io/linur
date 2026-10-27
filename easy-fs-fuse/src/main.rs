// SPDX-License-Identifier: GPL-3.0-only
use clap::{App, Arg};
use easy_fs::{EasyFileSystem, S_IFCHR, block_cache_sync_all, makedev};
use easy_fs_host::BlockFile;
use std::fs::{File, read_dir};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

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
    // 16MiB, at most 4095 files
    let block_file = Arc::new(BlockFile::create(target_path.join("fs.img"), 16 * 2048)?);
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
    // an image that is not flushed is not an image
    block_cache_sync_all();
    Ok(())
}
