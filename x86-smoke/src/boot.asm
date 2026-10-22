// SPDX-License-Identifier: GPL-3.0-only
// Boot chain for the x86_64 smoke kernel: MBR, 16-bit stage, protected mode,
// long mode. See docs/x86_64-smoke.md sections 2 and 3.
//
// Sector 0 is the MBR linked at 0x7c00; it reads the payload (everything below
// from _payload on) into 0x7e00 and jumps there.

    .section .mbr, "ax"
    .code16
    .globl _start
_start:
    cli
    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs
    movw $0x7c00, %sp
    cld

    // One extended read of 32 sectors from LBA 1 to 0000:7e00.
    movw $dap, %si
    movb $0x42, %ah
    int $0x13
    jc 1f
    ljmp $0x0000, $0x7e00

1:
    hlt
    jmp 1b

    .balign 4
dap:
    .byte 0x10, 0x00        // size, reserved
    .word 32                // sector count
    .word 0x7e00            // buffer offset
    .word 0x0000            // buffer segment
    .quad 1                 // LBA
    .quad 0

    .org 510
    .word 0xaa55

    .section .text.boot, "ax"
    .code16
    .globl _payload
_payload:
    // Fast A20 gate on port 0x92; keep the reset bit clear.
    inb $0x92, %al
    orb $0x02, %al
    andb $0xfe, %al
    outb %al, $0x92

    lgdt gdtr
    movl %cr0, %eax
    orl $0x1, %eax
    movl %eax, %cr0
    ljmp $0x08, $pm32

    .code32
pm32:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs

    // Zero PML4, PDPT and PD.
    movl $__pml4, %edi
    xorl %eax, %eax
    movl $(3 * 4096 / 4), %ecx
    rep stosl

    // PML4[0] -> PDPT[0] -> PD[0] = one present, writable 2 MiB page at 0.
    movl $__pdpt, %eax
    orl $0x3, %eax
    movl %eax, __pml4
    movl $__pd, %eax
    orl $0x3, %eax
    movl %eax, __pdpt
    movl $0x83, __pd

    movl $__pml4, %eax
    movl %eax, %cr3
    movl %cr4, %eax
    orl $(1 << 5), %eax
    movl %eax, %cr4

    movl $0xc0000080, %ecx
    rdmsr
    orl $(1 << 8), %eax
    wrmsr

    movl %cr0, %eax
    orl $(1 << 31), %eax
    movl %eax, %cr0
    ljmp $0x18, $long64

    .code64
long64:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs

    movq $__stack_top, %rsp
    call rust_main

1:
    hlt
    jmp 1b

    .balign 8
gdt:
    .quad 0x0000000000000000    // null
    .quad 0x00cf9a000000ffff    // 0x08: 32-bit code, flat 4 GiB
    .quad 0x00cf92000000ffff    // 0x10: flat data
    .quad 0x00af9a000000ffff    // 0x18: 64-bit code
gdt_end:
gdtr:
    .word gdt_end - gdt - 1
    .long gdt
