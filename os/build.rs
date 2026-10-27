// SPDX-License-Identifier: GPL-3.0-only
//! Compile the `linur-io/alloc` C module for the kernel's own target and hand the
//! object to rust-lld (docs/alloc.md §5). No `cc` crate: the module is a submodule
//! with its own tests and CI.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

static TARGET_PATH: &str = "../user/target/riscv64gc-unknown-none-elf/release/";

/// Compilers to try in order: `clang` is not always installed unversioned.
const CLANG: [&str; 6] = [
    "clang", "clang-19", "clang-20", "clang-21", "clang-22", "clang-18",
];

fn main() {
    println!("cargo:rerun-if-changed=../user/src/");
    println!("cargo:rerun-if-changed={}", TARGET_PATH);

    let submodule = Path::new(env!("CARGO_MANIFEST_DIR")).join("../alloc");
    let source = submodule.join("src/frame.c");
    if !source.exists() {
        panic!(
            "{} is missing: run `git submodule update --init` (docs/alloc.md §6)",
            source.display()
        );
    }
    println!("cargo:rerun-if-changed={}", submodule.display());
    println!("cargo:rerun-if-env-changed=CLANG");

    let object = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("frame.o");
    let clang = compiler();
    let mut cc = Command::new(&clang);
    cc.args([
        "-std=c17",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-ffreestanding",
        "-fno-stack-protector",
        "-fno-pic",
    ])
    .arg("-I")
    .arg(submodule.join("include"))
    .arg("-c")
    .arg(&source)
    .arg("-o")
    .arg(&object);
    match std::env::var("TARGET").unwrap().as_str() {
        "riscv64gc-unknown-none-elf" => {
            cc.args([
                "--target=riscv64-unknown-elf",
                "-march=rv64gc",
                "-mabi=lp64d",
                "-mcmodel=medany",
            ]);
        }
        "x86_64-unknown-none" => {
            cc.args(["--target=x86_64-unknown-none", "-mno-red-zone"]);
        }
        target => panic!("no clang flags for target {target} (docs/alloc.md §5)"),
    }
    let status = cc
        .status()
        .unwrap_or_else(|e| panic!("cannot run {}: {e}", clang.display()));
    assert!(
        status.success(),
        "clang failed to compile {}",
        source.display()
    );
    println!("cargo:rustc-link-arg={}", object.display());
}

/// The first compiler that answers `--version`, honouring `$CLANG`.
fn compiler() -> PathBuf {
    if let Ok(clang) = std::env::var("CLANG") {
        return PathBuf::from(clang);
    }
    CLANG
        .iter()
        .map(PathBuf::from)
        .find(|candidate| {
            Command::new(candidate)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        })
        .unwrap_or_else(|| panic!("no clang found: install one or set CLANG (docs/alloc.md §5)"))
}
