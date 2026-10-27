// SPDX-License-Identifier: GPL-3.0-only
use super::{FrameTracker, frame_alloc};
use super::{PTEFlags, PageTable, PageTableEntry};
use super::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use super::{StepByOne, VPNRange};
use crate::config::{
    MEMORY_END, MMIO, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT_BASE, USER_HEAP_BASE, USER_STACK_SIZE,
};
use crate::sync::UPSafeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use lazy_static::*;
use riscv::register::satp;

/// `e_machine` of RISC-V, the machine this kernel runs (docs/elf.md §3).
const EM_RISCV: u16 = 243;
/// `e_type` of a static executable; a PIE image (3) is not supported.
const ET_EXEC: u16 = 2;

unsafe extern "C" {
    safe fn stext();
    safe fn etext();
    safe fn srodata();
    safe fn erodata();
    safe fn sdata();
    safe fn edata();
    safe fn sbss_with_stack();
    safe fn ebss();
    safe fn ekernel();
    safe fn strampoline();
}

lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UPSafeCell<MemorySet>> =
        Arc::new(unsafe { UPSafeCell::new(MemorySet::new_kernel()) });
}

pub fn kernel_token() -> usize {
    KERNEL_SPACE.exclusive_access().token()
}

pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
    /// Lowest virtual address the brk heap may use.
    heap_bottom: usize,
    /// First byte above the brk heap, stored exactly as the user set it.
    program_brk: usize,
}

impl MemorySet {
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
            heap_bottom: 0,
            program_brk: 0,
        }
    }
    pub fn token(&self) -> usize {
        self.page_table.token()
    }
    /// Assume that no conflicts.
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        );
    }
    pub fn remove_area_with_start_vpn(&mut self, start_vpn: VirtPageNum) {
        if let Some((idx, area)) = self
            .areas
            .iter_mut()
            .enumerate()
            .find(|(_, area)| area.vpn_range.get_start() == start_vpn)
        {
            area.unmap(&mut self.page_table);
            self.areas.remove(idx);
        }
    }
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&self.page_table, data);
        }
        self.areas.push(map_area);
    }
    /// Mention that trampoline is not collected by areas.
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(linker_symbol_addr!(strampoline)).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }
    /// Without kernel stacks.
    pub fn new_kernel() -> Self {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // map kernel sections
        println!(
            ".text [{:#x}, {:#x})",
            linker_symbol_addr!(stext),
            linker_symbol_addr!(etext)
        );
        println!(
            ".rodata [{:#x}, {:#x})",
            linker_symbol_addr!(srodata),
            linker_symbol_addr!(erodata)
        );
        println!(
            ".data [{:#x}, {:#x})",
            linker_symbol_addr!(sdata),
            linker_symbol_addr!(edata)
        );
        println!(
            ".bss [{:#x}, {:#x})",
            linker_symbol_addr!(sbss_with_stack),
            linker_symbol_addr!(ebss)
        );
        println!("mapping .text section");
        memory_set.push(
            MapArea::new(
                (linker_symbol_addr!(stext)).into(),
                (linker_symbol_addr!(etext)).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::X,
            ),
            None,
        );
        println!("mapping .rodata section");
        memory_set.push(
            MapArea::new(
                (linker_symbol_addr!(srodata)).into(),
                (linker_symbol_addr!(erodata)).into(),
                MapType::Identical,
                MapPermission::R,
            ),
            None,
        );
        println!("mapping .data section");
        memory_set.push(
            MapArea::new(
                (linker_symbol_addr!(sdata)).into(),
                (linker_symbol_addr!(edata)).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        println!("mapping .bss section");
        memory_set.push(
            MapArea::new(
                (linker_symbol_addr!(sbss_with_stack)).into(),
                (linker_symbol_addr!(ebss)).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        println!("mapping physical memory");
        memory_set.push(
            MapArea::new(
                (linker_symbol_addr!(ekernel)).into(),
                MEMORY_END.into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        println!("mapping memory-mapped registers");
        for pair in MMIO {
            memory_set.push(
                MapArea::new(
                    pair.0.into(),
                    (pair.0 + pair.1).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            );
        }
        memory_set
    }
    /// Include sections in elf and trampoline, also returns user_sp_base and
    /// entry point. `None` when `elf_data` is not a loadable image: a bad image
    /// is an error return, never a panic.
    pub fn from_elf(elf_data: &[u8]) -> Option<(Self, usize, usize)> {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        // map the loadable segments of an image this machine can run (docs/elf.md §3)
        let elf = elf::parse(elf_data).ok()?;
        // F2: ELF, static, non-PIE; the x86_64 machine arrives at M5.4
        if elf.machine() != EM_RISCV || elf.kind() != ET_EXEC {
            return None;
        }
        // a dynamically linked image needs its interpreter, which v1.0 has none of
        if elf.has_interpreter() {
            return None;
        }
        let mut max_end_vpn = VirtPageNum(0);
        let mut segments = 0;
        for segment in elf.segments() {
            let start: usize = segment.vaddr;
            let end = start.checked_add(segment.memsz)?;
            // A loadable image must end below the trap context page, which sits
            // above the user stack, and must not be empty.
            if end <= start || end > TRAP_CONTEXT_BASE - PAGE_SIZE - USER_STACK_SIZE {
                return None;
            }
            let mut map_perm = MapPermission::U;
            if segment.read {
                map_perm |= MapPermission::R;
            }
            if segment.write {
                map_perm |= MapPermission::W;
            }
            if segment.execute {
                map_perm |= MapPermission::X;
            }
            let map_area = MapArea::new(start.into(), end.into(), MapType::Framed, map_perm);
            // Segments must not overlap: mapping one page twice is a bug.
            if map_area.vpn_range.get_start() < max_end_vpn {
                return None;
            }
            max_end_vpn = map_area.vpn_range.get_end();
            memory_set.push(map_area, Some(segment.data));
            segments += 1;
        }
        if segments == 0 {
            return None;
        }
        let max_end_va: VirtAddr = max_end_vpn.into();
        let mut user_stack_base: usize = max_end_va.into();
        user_stack_base += PAGE_SIZE;
        // The heap starts empty at a fixed address and grows through brk.
        memory_set.heap_bottom = USER_HEAP_BASE;
        memory_set.program_brk = USER_HEAP_BASE;
        memory_set.push(
            MapArea::new(
                USER_HEAP_BASE.into(),
                USER_HEAP_BASE.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        Some((memory_set, user_stack_base, elf.entry()))
    }
    pub fn from_existed_user(user_space: &MemorySet) -> MemorySet {
        let mut memory_set = Self::new_bare();
        // map trampoline
        memory_set.map_trampoline();
        memory_set.heap_bottom = user_space.heap_bottom;
        memory_set.program_brk = user_space.program_brk;
        // Copy only the frames an area really maps: munmap leaves holes, and a
        // hole must stay a hole in the child instead of becoming a fresh frame.
        for area in user_space.areas.iter() {
            let mut new_area = MapArea::from_another(area);
            for (&vpn, src_frame) in area.data_frames.iter() {
                new_area.map_one(&mut memory_set.page_table, vpn);
                new_area.data_frames[&vpn]
                    .ppn
                    .get_bytes_array()
                    .copy_from_slice(src_frame.ppn.get_bytes_array());
            }
            memory_set.areas.push(new_area);
        }
        memory_set
    }
    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe {
            satp::write(satp::Satp::from_bits(satp));
            asm!("sfence.vma");
        }
    }
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }
    /// Map [start, end) as a framed area if every page there is unmapped.
    pub fn mmap(&mut self, start: VirtAddr, end: VirtAddr, permission: MapPermission) -> bool {
        let vpn_range = VPNRange::new(start.floor(), end.ceil());
        for vpn in vpn_range {
            if let Some(pte) = self.translate(vpn)
                && pte.is_valid()
            {
                return false;
            }
        }
        self.push(MapArea::new(start, end, MapType::Framed, permission), None);
        true
    }
    /// Unmap [start, end). Every page is validated before the first unmap, so a
    /// failure leaves the whole range mapped.
    pub fn munmap(&mut self, start: VirtAddr, end: VirtAddr) -> bool {
        let vpn_range = VPNRange::new(start.floor(), end.ceil());
        for vpn in vpn_range {
            if self.translate(vpn).is_none_or(|pte| !pte.is_valid()) {
                return false;
            }
        }
        for vpn in vpn_range {
            let Some(area) = self.areas.iter_mut().find(|area| area.contains_mapped(vpn)) else {
                return false;
            };
            area.unmap_one(&mut self.page_table, vpn);
        }
        // The kernel page table is active, so the user ASID may cache stale pages.
        unsafe { asm!("sfence.vma") };
        true
    }
    /// Set the program break: 0 returns the current break, otherwise grow or
    /// shrink the heap to addr. None when addr is below the heap bottom or the
    /// new pages would overlap another mapping; a refused call changes nothing.
    pub fn brk(&mut self, addr: usize) -> Option<usize> {
        if addr == 0 {
            return Some(self.program_brk);
        }
        if addr < self.heap_bottom {
            return None;
        }
        let old_end = VirtAddr::from(self.program_brk).ceil();
        let new_end = VirtAddr::from(addr).ceil();
        if new_end > old_end {
            // Validate the whole range before mapping the first page.
            for vpn in VPNRange::new(old_end, new_end) {
                if let Some(pte) = self.translate(vpn)
                    && pte.is_valid()
                {
                    return None;
                }
            }
        }
        let heap_start = VirtAddr::from(self.heap_bottom).floor();
        let area = self
            .areas
            .iter_mut()
            .find(|area| area.vpn_range.get_start() == heap_start)?;
        if new_end > old_end {
            area.map_range(&mut self.page_table, old_end, new_end);
        } else if new_end < old_end {
            area.unmap_range(&mut self.page_table, new_end, old_end);
        }
        if new_end != old_end {
            unsafe { asm!("sfence.vma") };
        }
        self.program_brk = addr;
        Some(addr)
    }
    pub fn recycle_data_pages(&mut self) {
        //*self = Self::new_bare();
        self.areas.clear();
    }
}

pub struct MapArea {
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }
    pub fn from_another(another: &MapArea) -> Self {
        Self {
            vpn_range: VPNRange::new(another.vpn_range.get_start(), another.vpn_range.get_end()),
            data_frames: BTreeMap::new(),
            map_type: another.map_type,
            map_perm: another.map_perm,
        }
    }
    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn: PhysPageNum;
        match self.map_type {
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0);
            }
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
            }
        }
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap();
        page_table.map(vpn, ppn, pte_flags);
    }
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }
    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }
    /// Unmap the whole area, skipping holes left by munmap.
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            if self.contains_mapped(vpn) {
                self.unmap_one(page_table, vpn);
            }
        }
    }
    /// Map [start_vpn, end_vpn) and extend the area to end_vpn; the caller
    /// checked that every page there is free.
    fn map_range(
        &mut self,
        page_table: &mut PageTable,
        start_vpn: VirtPageNum,
        end_vpn: VirtPageNum,
    ) {
        for vpn in VPNRange::new(start_vpn, end_vpn) {
            self.map_one(page_table, vpn);
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), end_vpn);
    }
    /// Free [start_vpn, end_vpn) and shrink the area to start_vpn, skipping
    /// holes left by munmap.
    fn unmap_range(
        &mut self,
        page_table: &mut PageTable,
        start_vpn: VirtPageNum,
        end_vpn: VirtPageNum,
    ) {
        for vpn in VPNRange::new(start_vpn, end_vpn) {
            if self.contains_mapped(vpn) {
                self.unmap_one(page_table, vpn);
            }
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), start_vpn);
    }
    /// Whether a page of the area is actually mapped: a Framed area may keep
    /// holes after munmap.
    fn contains_mapped(&self, vpn: VirtPageNum) -> bool {
        match self.map_type {
            MapType::Identical => {
                self.vpn_range.get_start() <= vpn && vpn < self.vpn_range.get_end()
            }
            MapType::Framed => self.data_frames.contains_key(&vpn),
        }
    }
    /// data: start-aligned but maybe with shorter length
    /// assume that all frames were cleared before
    pub fn copy_data(&mut self, page_table: &PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step();
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MapType {
    Identical,
    Framed,
}

bitflags! {
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}

#[allow(unused)]
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((linker_symbol_addr!(stext) + linker_symbol_addr!(etext)) / 2).into();
    let mid_rodata: VirtAddr =
        ((linker_symbol_addr!(srodata) + linker_symbol_addr!(erodata)) / 2).into();
    let mid_data: VirtAddr = ((linker_symbol_addr!(sdata) + linker_symbol_addr!(edata)) / 2).into();
    assert!(
        !kernel_space
            .page_table
            .translate(mid_text.floor())
            .unwrap()
            .writable(),
    );
    assert!(
        !kernel_space
            .page_table
            .translate(mid_rodata.floor())
            .unwrap()
            .writable(),
    );
    assert!(
        !kernel_space
            .page_table
            .translate(mid_data.floor())
            .unwrap()
            .executable(),
    );
    println!("remap_test passed!");
}
