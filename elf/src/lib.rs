// SPDX-License-Identifier: GPL-3.0-only
//! An allocation-free ELF64 parser: image structure only, no process policy.

#![no_std]
#![deny(missing_docs)]

/// The ELF64 file header is 64 bytes.
const EHDR_SIZE: usize = 64;
/// The ELF64 program header is 56 bytes.
const PHDR_SIZE: usize = 56;
/// A segment the loader maps.
const PT_LOAD: u32 = 1;
/// A dynamically linked image names its interpreter here.
const PT_INTERP: u32 = 3;
/// Segment is executable (PF_X).
const PF_X: u32 = 1;
/// Segment is writable (PF_W).
const PF_W: u32 = 2;
/// Segment is readable (PF_R).
const PF_R: u32 = 4;

/// Why an image was refused. Every variant maps to ENOEXEC in the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// No 0x7f ELF magic.
    NotElf,
    /// EI_CLASS is not 2 (ELF64).
    UnsupportedClass,
    /// EI_DATA is not 1 (little endian).
    UnsupportedEncoding,
    /// Shorter than the 64-byte ELF header (an empty image included).
    Truncated,
    /// e_version, e_ehsize or e_phentsize is wrong.
    BadHeader,
    /// The program-header table lies outside the image.
    BadProgramHeaders,
    /// filesz > memsz, the segment bytes run past the image, or the address overflows.
    BadSegment,
}

/// One PT_LOAD segment, borrowing the image bytes.
pub struct Segment<'a> {
    /// Destination virtual address (p_vaddr).
    pub vaddr: usize,
    /// Memory size; the loader zero-fills the tail beyond data.len() (p_memsz).
    pub memsz: usize,
    /// The p_filesz file bytes to copy at vaddr.
    pub data: &'a [u8],
    /// p_flags has PF_R.
    pub read: bool,
    /// p_flags has PF_W.
    pub write: bool,
    /// p_flags has PF_X.
    pub execute: bool,
}

/// A validated ELF64 image.
pub struct Elf<'a> {
    image: &'a [u8],
    machine: u16,
    kind: u16,
    entry: usize,
    phoff: usize,
    phnum: usize,
}

/// Parse and validate an ELF64 little-endian image (docs/elf.md §2).
pub fn parse(image: &[u8]) -> Result<Elf<'_>, ElfError> {
    if image.len() < EHDR_SIZE {
        return Err(ElfError::Truncated);
    }
    if image[..4] != [0x7f, b'E', b'L', b'F'] {
        return Err(ElfError::NotElf);
    }
    if image[4] != 2 {
        return Err(ElfError::UnsupportedClass);
    }
    if image[5] != 1 {
        return Err(ElfError::UnsupportedEncoding);
    }
    if u32_at(image, 20) != 1
        || u16_at(image, 52) != EHDR_SIZE as u16
        || u16_at(image, 54) != PHDR_SIZE as u16
    {
        return Err(ElfError::BadHeader);
    }
    let phoff = usize::try_from(u64_at(image, 32)).map_err(|_| ElfError::BadProgramHeaders)?;
    let phnum = usize::from(u16_at(image, 56));
    let table_end = phnum
        .checked_mul(PHDR_SIZE)
        .and_then(|size| phoff.checked_add(size))
        .ok_or(ElfError::BadProgramHeaders)?;
    if table_end > image.len() {
        return Err(ElfError::BadProgramHeaders);
    }

    let elf = Elf {
        image,
        machine: u16_at(image, 18),
        kind: u16_at(image, 16),
        entry: u64_at(image, 24) as usize,
        phoff,
        phnum,
    };
    for header in elf.headers() {
        if u32_at(header, 0) == PT_LOAD {
            segment(image, header)?;
        }
    }
    Ok(elf)
}

impl<'a> Elf<'a> {
    /// e_machine: 243 is RISC-V, 62 is x86_64.
    pub fn machine(&self) -> u16 {
        self.machine
    }

    /// e_type: 2 is ET_EXEC, the only kind the kernel runs.
    pub fn kind(&self) -> u16 {
        self.kind
    }

    /// e_entry, the first instruction executed.
    pub fn entry(&self) -> usize {
        self.entry
    }

    /// Whether the image names an interpreter (`PT_INTERP`), i.e. it is
    /// dynamically linked; this kernel does not run such an image (docs/elf.md §1).
    pub fn has_interpreter(&self) -> bool {
        self.headers().any(|header| u32_at(header, 0) == PT_INTERP)
    }

    /// The loadable segments in program-header order.
    pub fn segments(&self) -> impl Iterator<Item = Segment<'a>> + '_ {
        let image = self.image;
        self.headers()
            .filter(|header| u32_at(header, 0) == PT_LOAD)
            // parse() already validated every PT_LOAD header.
            .filter_map(move |header| segment(image, header).ok())
    }

    /// The program headers, in file order.
    fn headers(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        let image = self.image;
        let phoff = self.phoff;
        (0..self.phnum).map(move |i| &image[phoff + i * PHDR_SIZE..phoff + (i + 1) * PHDR_SIZE])
    }
}

/// Build one PT_LOAD segment, validating its fields.
fn segment<'a>(image: &'a [u8], header: &'a [u8]) -> Result<Segment<'a>, ElfError> {
    let offset = usize::try_from(u64_at(header, 8)).map_err(|_| ElfError::BadSegment)?;
    let vaddr = usize::try_from(u64_at(header, 16)).map_err(|_| ElfError::BadSegment)?;
    let filesz = usize::try_from(u64_at(header, 32)).map_err(|_| ElfError::BadSegment)?;
    let memsz = usize::try_from(u64_at(header, 40)).map_err(|_| ElfError::BadSegment)?;
    let end = offset.checked_add(filesz).ok_or(ElfError::BadSegment)?;
    if filesz > memsz || end > image.len() {
        return Err(ElfError::BadSegment);
    }
    vaddr.checked_add(memsz).ok_or(ElfError::BadSegment)?;
    let flags = u32_at(header, 4);
    Ok(Segment {
        vaddr,
        memsz,
        data: &image[offset..end],
        read: flags & PF_R != 0,
        write: flags & PF_W != 0,
        execute: flags & PF_X != 0,
    })
}

/// Read the little-endian u16 at `off`.
fn u16_at(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([bytes[off], bytes[off + 1]])
}

/// Read the little-endian u32 at `off`.
fn u32_at(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

/// Read the little-endian u64 at `off`.
fn u64_at(bytes: &[u8], off: usize) -> u64 {
    let mut word = [0; 8];
    word.copy_from_slice(&bytes[off..off + 8]);
    u64::from_le_bytes(word)
}
