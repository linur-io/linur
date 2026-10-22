/* SPDX-License-Identifier: GPL-3.0-only */
/* Image layout: MBR at 0x7c00, payload at 0x7e00. See docs/x86_64-smoke.md section 2. */
ENTRY(_start)
SECTIONS
{
    . = 0x7c00;
    .mbr : {
        KEEP(*(.mbr))
    }

    . = 0x7e00;
    .text : {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }
    .rodata : {
        *(.rodata .rodata.*)
    }
    .data : {
        *(.data .data.*)
    }

    . = ALIGN(4096);
    .bss (NOLOAD) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4096);
        __pml4 = .;
        . += 4096;
        __pdpt = .;
        . += 4096;
        __pd = .;
        . += 4096;
        . = ALIGN(16);
        __stack_bottom = .;
        . += 16384;
        __stack_top = .;
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame .eh_frame_hdr)
        *(.note .note.*)
        *(.comment)
        *(.llvm_addrsig)
    }
}
