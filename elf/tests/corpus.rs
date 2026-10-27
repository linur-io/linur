// SPDX-License-Identifier: GPL-3.0-only
//! Host corpus for the ELF64 parser: docs/elf.md §7.

use elf::{ElfError, parse};

const EHDR_SIZE: usize = 64;
const PHDR_SIZE: usize = 56;
const PHOFF: usize = EHDR_SIZE;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const MACHINE_RISCV: u16 = 243;
const MACHINE_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;
const PT_NOTE: u32 = 4;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

/// A program header, copied verbatim into the image.
struct Phdr {
    p_type: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
}

impl Phdr {
    /// A PT_LOAD header for filesz bytes at offset, in memsz bytes at vaddr.
    fn load(offset: u64, vaddr: u64, filesz: u64, memsz: u64, flags: u32) -> Self {
        Self {
            p_type: PT_LOAD,
            flags,
            offset,
            vaddr,
            filesz,
            memsz,
        }
    }

    /// A PT_INTERP header; the parser reports it, the kernel refuses the image.
    fn interp(offset: u64, filesz: u64) -> Self {
        Self {
            p_type: PT_INTERP,
            flags: 0,
            offset,
            vaddr: 0,
            filesz,
            memsz: filesz,
        }
    }

    /// A PT_NOTE header whose fields would fail a PT_LOAD check; it must be ignored.
    fn note() -> Self {
        Self {
            p_type: PT_NOTE,
            flags: 0,
            offset: 0x1000,
            vaddr: 0x1000,
            filesz: 0x1000,
            memsz: 0,
        }
    }
}

/// Build an ELF64 image: the 64-byte header, the program headers at PHOFF, then
/// payload (segment bytes) at payload_offset(phdrs.len()).
fn image(phdrs: &[Phdr], payload: &[u8]) -> Vec<u8> {
    let mut image = vec![0u8; PHOFF + phdrs.len() * PHDR_SIZE];
    image[..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[6] = 1;
    put16(&mut image, 16, ET_EXEC);
    put16(&mut image, 18, MACHINE_RISCV);
    put32(&mut image, 20, 1);
    put64(&mut image, 24, 0x1000);
    put64(&mut image, 32, PHOFF as u64);
    put16(&mut image, 52, EHDR_SIZE as u16);
    put16(&mut image, 54, PHDR_SIZE as u16);
    put16(&mut image, 56, phdrs.len() as u16);
    for (i, ph) in phdrs.iter().enumerate() {
        let base = PHOFF + i * PHDR_SIZE;
        put32(&mut image, base, ph.p_type);
        put32(&mut image, base + 4, ph.flags);
        put64(&mut image, base + 8, ph.offset);
        put64(&mut image, base + 16, ph.vaddr);
        put64(&mut image, base + 32, ph.filesz);
        put64(&mut image, base + 40, ph.memsz);
    }
    image.extend_from_slice(payload);
    image
}

/// File offset of the payload in an image with phnum program headers.
fn payload_offset(phnum: usize) -> u64 {
    (PHOFF + phnum * PHDR_SIZE) as u64
}

/// A valid image: one R+X PT_LOAD of 16 bytes at 0x1000.
fn valid() -> Vec<u8> {
    image(
        &[Phdr::load(payload_offset(1), 0x1000, 16, 16, PF_R | PF_X)],
        &[0xa5; 16],
    )
}

/// Parse the image and return the refusal it must produce.
fn refused(image: &[u8]) -> ElfError {
    match parse(image) {
        Ok(_) => panic!("image was accepted"),
        Err(err) => err,
    }
}

fn put16(image: &mut [u8], at: usize, value: u16) {
    image[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(image: &mut [u8], at: usize, value: u32) {
    image[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(image: &mut [u8], at: usize, value: u64) {
    image[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn valid_image_reports_header_and_segment() {
    let image = valid();
    let elf = parse(&image).expect("valid image");
    assert_eq!(elf.machine(), MACHINE_RISCV);
    assert_eq!(elf.kind(), ET_EXEC);
    assert_eq!(elf.entry(), 0x1000);
    let segments: Vec<_> = elf.segments().collect();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].vaddr, 0x1000);
    assert_eq!(segments[0].memsz, 16);
    assert_eq!(segments[0].data, &[0xa5; 16]);
    assert!(segments[0].read);
    assert!(!segments[0].write);
    assert!(segments[0].execute);
}

#[test]
fn two_segments_with_bss_tail() {
    let mut payload = vec![0x11; 16];
    payload.extend_from_slice(&[0x22; 8]);
    let image = image(
        &[
            Phdr::load(payload_offset(2), 0x400000, 16, 16, PF_R | PF_X),
            Phdr::load(payload_offset(2) + 16, 0x600000, 8, 24, PF_R | PF_W),
        ],
        &payload,
    );
    let elf = parse(&image).expect("valid image");
    let segments: Vec<_> = elf.segments().collect();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].vaddr, 0x400000);
    assert_eq!(segments[0].data, &[0x11; 16]);
    assert!(segments[0].read && segments[0].execute && !segments[0].write);
    assert_eq!(segments[1].vaddr, 0x600000);
    assert_eq!(segments[1].memsz, 24);
    assert_eq!(segments[1].data, &[0x22; 8]);
    assert!(segments[1].read && segments[1].write && !segments[1].execute);
}

#[test]
fn non_load_header_is_ignored() {
    let image = image(
        &[
            Phdr::note(),
            Phdr::load(payload_offset(3), 0x1000, 16, 16, PF_R),
            Phdr::note(),
        ],
        &[0xa5; 16],
    );
    let elf = parse(&image).expect("valid image");
    let segments: Vec<_> = elf.segments().collect();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].vaddr, 0x1000);
}

#[test]
fn no_pt_load_has_no_segments() {
    let image = image(&[Phdr::note()], &[]);
    let elf = parse(&image).expect("valid image");
    assert_eq!(elf.segments().count(), 0);
}

#[test]
fn no_program_headers_has_no_segments() {
    let image = image(&[], &[]);
    let elf = parse(&image).expect("valid image");
    assert_eq!(elf.segments().count(), 0);
}

#[test]
fn foreign_machine_is_left_to_the_kernel() {
    let mut image = valid();
    put16(&mut image, 18, MACHINE_X86_64);
    let elf = parse(&image).expect("parser does not check the machine");
    assert_eq!(elf.machine(), MACHINE_X86_64);
}

#[test]
fn et_dyn_is_left_to_the_kernel() {
    let mut image = valid();
    put16(&mut image, 16, ET_DYN);
    let elf = parse(&image).expect("parser does not check e_type");
    assert_eq!(elf.kind(), ET_DYN);
}

#[test]
fn no_magic_is_not_elf() {
    let mut image = valid();
    image[0] = 0;
    assert_eq!(refused(&image), ElfError::NotElf);
}

#[test]
fn elf32_class_is_unsupported() {
    let mut image = valid();
    image[4] = 1;
    assert_eq!(refused(&image), ElfError::UnsupportedClass);
}

#[test]
fn big_endian_is_unsupported() {
    let mut image = valid();
    image[5] = 2;
    assert_eq!(refused(&image), ElfError::UnsupportedEncoding);
}

#[test]
fn truncated_header_is_refused() {
    assert_eq!(refused(&valid()[..EHDR_SIZE - 1]), ElfError::Truncated);
    assert_eq!(refused(&[]), ElfError::Truncated);
}

#[test]
fn phdr_table_past_eof_is_refused() {
    let mut image = valid();
    put64(&mut image, 32, 0x1000);
    assert_eq!(refused(&image), ElfError::BadProgramHeaders);
}

#[test]
fn wrong_phentsize_is_refused() {
    let mut image = valid();
    put16(&mut image, 54, 32);
    assert_eq!(refused(&image), ElfError::BadHeader);
}

#[test]
fn wrong_version_is_refused() {
    let mut image = valid();
    put32(&mut image, 20, 2);
    assert_eq!(refused(&image), ElfError::BadHeader);
}

#[test]
fn wrong_ehsize_is_refused() {
    let mut image = valid();
    put16(&mut image, 52, 32);
    assert_eq!(refused(&image), ElfError::BadHeader);
}

#[test]
fn filesz_beyond_memsz_is_refused() {
    let image = image(
        &[Phdr::load(payload_offset(1), 0x1000, 24, 16, PF_R)],
        &[0; 24],
    );
    assert_eq!(refused(&image), ElfError::BadSegment);
}

#[test]
fn segment_past_eof_is_refused() {
    let image = image(
        &[Phdr::load(payload_offset(1), 0x1000, 24, 24, PF_R)],
        &[0; 16],
    );
    assert_eq!(refused(&image), ElfError::BadSegment);
}

#[test]
fn vaddr_overflow_is_refused() {
    let image = image(
        &[Phdr::load(payload_offset(1), u64::MAX, 16, 16, PF_R)],
        &[0; 16],
    );
    assert_eq!(refused(&image), ElfError::BadSegment);
}

#[test]
fn interpreter_is_reported() {
    // A dynamically linked image parses: the parser reports the interpreter and
    // the kernel is the one that refuses to run it (docs/elf.md §1).
    let payload = payload_offset(2);
    let image = image(
        &[
            Phdr::interp(payload, 4),
            Phdr::load(payload, 0x1000, 16, 16, PF_R | PF_X),
        ],
        &[0; 16],
    );
    let elf = parse(&image).expect("a PT_INTERP image is valid ELF");
    assert!(elf.has_interpreter());
    assert_eq!(elf.segments().count(), 1);
}

#[test]
fn a_static_image_has_no_interpreter() {
    let image = valid();
    let elf = parse(&image).expect("valid image");
    assert!(!elf.has_interpreter());
}
