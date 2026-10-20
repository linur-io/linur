// SPDX-License-Identifier: GPL-3.0-only
use clap::{App, Arg};
use easy_fs::{BlockDevice, EasyFileSystem};
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
        let inode = root_inode.create(name.as_str()).unwrap();
        // write data to easy-fs
        inode.write_at(0, all_data.as_slice());
    }
    Ok(())
}

#[test]
fn efs_test() -> std::io::Result<()> {
    let block_file = Arc::new(BlockFile(Mutex::new({
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("target/fs.img")?;
        f.set_len(8192 * 512).unwrap();
        f
    })));
    EasyFileSystem::create(block_file.clone(), 4096, 1);
    let efs = EasyFileSystem::open(block_file.clone());
    let root_inode = EasyFileSystem::root_inode(&efs);
    root_inode.create("filea");
    root_inode.create("fileb");
    for name in root_inode.ls() {
        println!("{}", name);
    }
    let filea = root_inode.find("filea").unwrap();
    let greet_str = "Hello, world!";
    filea.write_at(0, greet_str.as_bytes());
    //let mut buffer = [0u8; 512];
    let mut buffer = [0u8; 233];
    let len = filea.read_at(0, &mut buffer);
    assert_eq!(greet_str, core::str::from_utf8(&buffer[..len]).unwrap(),);

    let mut random_str_test = |len: usize| {
        filea.clear();
        assert_eq!(filea.read_at(0, &mut buffer), 0,);
        let mut str = String::new();
        use rand;
        // random digit
        for _ in 0..len {
            str.push(char::from('0' as u8 + rand::random::<u8>() % 10));
        }
        filea.write_at(0, str.as_bytes());
        let mut read_buffer = [0u8; 127];
        let mut offset = 0usize;
        let mut read_str = String::new();
        loop {
            let len = filea.read_at(offset, &mut read_buffer);
            if len == 0 {
                break;
            }
            offset += len;
            read_str.push_str(core::str::from_utf8(&read_buffer[..len]).unwrap());
        }
        assert_eq!(str, read_str);
    };

    random_str_test(4 * BLOCK_SZ);
    random_str_test(8 * BLOCK_SZ + BLOCK_SZ / 2);
    random_str_test(100 * BLOCK_SZ);
    random_str_test(70 * BLOCK_SZ + BLOCK_SZ / 7);
    random_str_test((12 + 128) * BLOCK_SZ);
    random_str_test(400 * BLOCK_SZ);
    random_str_test(1000 * BLOCK_SZ);
    random_str_test(2000 * BLOCK_SZ);

    Ok(())
}
