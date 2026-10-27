// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

extern crate alloc;

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use user_lib::{
    ENOENT, ENOEXEC, close, creat, errno, exec, execve, exit, fork, unlink, waitpid, write,
};

const NAME: &str = "elf_test";

/// The ELF64 file header and program header sizes.
const EHDR: usize = 64;
const PHDR: usize = 56;
/// A loadable user address; no image here reaches the page tables, because every
/// one of them is refused before it is mapped.
const VADDR: usize = 0x1_0000;
/// The kernel's ceiling `TRAP_CONTEXT_BASE - PAGE_SIZE - USER_STACK_SIZE`
/// (docs/elf.md §3), with the kernel's 4 KiB page and 8 KiB user stack.
const IMAGE_LIMIT: usize = usize::MAX - 3 * 4096 + 1 - 8192;

/// Counts expectations; a failed one prints which case broke, and the exit code
/// is 0 only when every one held.
struct Tally {
    passed: usize,
    total: usize,
}

impl Tally {
    const fn new() -> Self {
        Self {
            passed: 0,
            total: 0,
        }
    }

    fn check(&mut self, case: &str, ok: bool) {
        self.total += 1;
        if ok {
            self.passed += 1;
        } else {
            println!("{}: FAIL {}", NAME, case);
        }
    }

    fn result(&self) -> i32 {
        println!("{}: {}/{} passed", NAME, self.passed, self.total);
        if self.passed == self.total { 0 } else { 1 }
    }
}

/// A case label: the path without the NUL the syscall wrappers need.
fn plain(path: &str) -> &str {
    path.trim_end_matches('\0')
}

fn put16(image: &mut [u8], off: usize, value: u16) {
    image[off..off + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(image: &mut [u8], off: usize, value: u32) {
    image[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(image: &mut [u8], off: usize, value: u64) {
    image[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

/// A valid ELF64 image for this machine: one PT_LOAD of `payload` bytes at VADDR.
fn image(payload: usize) -> Vec<u8> {
    let mut elf = vec![0u8; EHDR + PHDR + payload];
    elf[..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2; // EI_CLASS: ELF64
    elf[5] = 1; // EI_DATA: little endian
    elf[6] = 1; // EI_VERSION
    put16(&mut elf, 16, 2); // e_type: ET_EXEC
    put16(&mut elf, 18, 243); // e_machine: EM_RISCV
    put32(&mut elf, 20, 1); // e_version
    put64(&mut elf, 24, VADDR as u64); // e_entry
    put64(&mut elf, 32, EHDR as u64); // e_phoff
    put16(&mut elf, 52, EHDR as u16); // e_ehsize
    put16(&mut elf, 54, PHDR as u16); // e_phentsize
    put16(&mut elf, 56, 1); // e_phnum
    put32(&mut elf, EHDR, 1); // p_type: PT_LOAD
    put32(&mut elf, EHDR + 4, 5); // p_flags: R | X
    put64(&mut elf, EHDR + 8, (EHDR + PHDR) as u64); // p_offset
    put64(&mut elf, EHDR + 16, VADDR as u64); // p_vaddr
    put64(&mut elf, EHDR + 32, payload as u64); // p_filesz
    put64(&mut elf, EHDR + 40, payload as u64); // p_memsz
    put64(&mut elf, EHDR + 48, 4096); // p_align
    elf
}

/// The defective images and the field each one breaks (docs/elf.md §6).
fn bad_images() -> Vec<(&'static str, Vec<u8>)> {
    let mut cases = Vec::new();
    let mut elf;

    // Not an ELF at all: an empty file, the literal "hello", a header cut short,
    // and a full-size image whose magic is wrong.
    cases.push(("et_empty\0", Vec::new()));
    cases.push(("et_hello\0", b"hello".to_vec()));
    cases.push(("et_short\0", image(4)[..32].to_vec()));
    elf = image(4);
    elf[..4].copy_from_slice(b"hell");
    cases.push(("et_magic\0", elf));

    // EI_CLASS 1 is ELF32.
    elf = image(4);
    elf[4] = 1;
    cases.push(("et_class32\0", elf));

    // The x86_64 machine, and ET_DYN: the kernel runs neither.
    elf = image(4);
    put16(&mut elf, 18, 62);
    cases.push(("et_x86_64\0", elf));
    elf = image(4);
    put16(&mut elf, 16, 3);
    cases.push(("et_dyn\0", elf));

    // The program-header table starts past the end of the file.
    elf = image(4);
    let past = elf.len() as u64 + 8;
    put64(&mut elf, 32, past);
    cases.push(("et_phoff\0", elf));

    // p_filesz beyond p_memsz, and p_offset + p_filesz beyond the file.
    elf = image(4);
    put64(&mut elf, EHDR + 32, 8);
    cases.push(("et_filesz\0", elf));
    elf = image(4);
    put64(&mut elf, EHDR + 8, 4096);
    cases.push(("et_offset\0", elf));

    // p_vaddr + p_memsz reaches past the last address an image may use.
    elf = image(4);
    put64(&mut elf, EHDR + 16, IMAGE_LIMIT as u64);
    cases.push(("et_vaddr\0", elf));

    // No program headers, so no segment to map.
    elf = image(4);
    put16(&mut elf, 56, 0);
    cases.push(("et_phnum0\0", elf));

    cases
}

/// Write `data` to `name`, replacing it; a failed write would hide the case.
fn write_file(name: &str, data: &[u8]) {
    let fd = creat(name, 0o755);
    assert!(fd >= 0, "create {}", plain(name));
    let fd = fd as usize;
    assert_eq!(
        write(fd, data),
        data.len() as isize,
        "write {}",
        plain(name)
    );
    close(fd);
}

/// Run `body` in a child process and return its exit code.
fn child(body: impl FnOnce() -> i32) -> i32 {
    let pid = fork();
    if pid == 0 {
        exit(body());
    }
    let mut code: i32 = 0;
    assert_eq!(waitpid(pid as usize, &mut code), pid);
    code
}

/// True when `exec(path)` in a child fails with `want`.
fn exec_fails(path: &str, want: i32) -> bool {
    child(|| {
        let ret = exec(path, &[core::ptr::null::<u8>()]);
        if ret == -1 && errno() == want { 0 } else { 1 }
    }) == 0
}

/// True when the image or script at `path` runs a child to exit code 0.
fn runs(path: &str) -> bool {
    child(|| {
        exec(path, &[core::ptr::null::<u8>()]);
        127
    }) == 0
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut t = Tally::new();

    // A malformed image is ENOEXEC, never a panic and never a partial load.
    for (name, data) in bad_images() {
        write_file(name, &data);
        t.check(
            &format!("exec {} ENOEXEC", plain(name)),
            exec_fails(name, ENOEXEC),
        );
        t.check(&format!("unlink {}", plain(name)), unlink(name) == 0);
    }

    // A #! script runs its interpreter; the rest of the line is one argument.
    write_file("et_script\0", b"#!/hello_world\n");
    t.check("#!/hello_world runs", runs("et_script\0"));
    write_file("et_arg\0", b"#!/hello_world 7\n");
    t.check("#!/hello_world 7 runs", runs("et_arg\0"));
    write_file("et_bad_script\0", b"#!/no_such_interp\n");
    t.check(
        "a missing interpreter ENOENT",
        exec_fails("et_bad_script\0", ENOENT),
    );

    // execve hands envp to the new image; exec passes none.
    let argv = [
        b"envp_child\0".as_ptr(),
        b"two\0".as_ptr(),
        core::ptr::null::<u8>(),
    ];
    let envp = [
        b"A=1\0".as_ptr(),
        b"B=2\0".as_ptr(),
        core::ptr::null::<u8>(),
    ];
    t.check(
        "execve passes A=1 and B=2",
        child(|| {
            execve("envp_child\0", &argv, &envp);
            127
        }) == 0,
    );
    let argv = [
        b"envp_child\0".as_ptr(),
        b"none\0".as_ptr(),
        core::ptr::null::<u8>(),
    ];
    t.check(
        "exec passes an empty envp",
        child(|| {
            exec("envp_child\0", &argv);
            127
        }) == 0,
    );

    t.check("unlink et_script", unlink("et_script\0") == 0);
    t.check("unlink et_arg", unlink("et_arg\0") == 0);
    t.check("unlink et_bad_script", unlink("et_bad_script\0") == 0);

    t.result()
}
