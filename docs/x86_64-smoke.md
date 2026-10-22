# linur — x86_64 smoke bring-up (M1.5)

Status: interface contract for M1.5. Read with `spec/02` (M1.5 deliverable),
`spec/00` D12 and `docs/hal.md`.

## 1. Scope and non-goals

Own loader → long mode → serial output → clean exit, booted in CI. Deliberately
absent: user mode, devices beyond the 16550 UART, interrupt handling (no IDT, no
PIC, no timer), an allocator, SMP, and any filesystem. The full port is M5.4; this
crate is a smoke test and must not grow into one.

The compile-only check `cargo check --target x86_64-unknown-none` on `hal` and
`x86-smoke` starts here and stays in the gate until M5.4.

## 2. Image

`x86-smoke/build/boot.img` is a raw disk of 33 sectors (16 896 bytes), booted as
the first hard disk:

| sectors | content | linked/loaded at |
|---|---|---|
| 0 | MBR: 16-bit loader, disk read, jump to the payload | `0x0000_7C00` |
| 1–32 | payload: 16/32/64-bit entry, GDT, page tables setup, Rust text/data | `0x0000_7E00` |

The payload is 32 sectors = 16 KiB, read by one `INT 13h AH=42h` extended read
into `0000:7E00`; that keeps the transfer below the 64 KiB real-mode offset
boundary. The build pads the image to 33 sectors and fails if the payload exceeds
16 KiB.

Physical memory the payload uses (all below 1 MiB and inside the 2 MiB page):

| range | use |
|---|---|
| `0x0000_7C00` | MBR |
| `0x0000_7E00` | payload `.text`, `.rodata`, `.data` |
| after `.data` | `.bss`: PML4 + PDPT + PD (12 KiB), 16 KiB boot stack |

## 3. Boot protocol

1. The BIOS loads sector 0 and jumps to `0000:7C00`.
2. The MBR zeroes the segments, sets `SS:SP = 0000:7C00`, disables interrupts,
   reads the payload from LBA 1 with `INT 13h AH=42h` (32 sectors, buffer
   `0000:7E00`), and jumps to the payload. A failed read halts.
3. The payload starts in 16-bit real mode: enables A20 (port `0x92`), loads the
   GDT (null; 32-bit code; flat data; 64-bit code) and sets `CR0.PE`.
4. 32-bit protected mode: zeroes the three page-table pages in `.bss`, builds
   PML4[0] → PDPT[0] → PD[0] as one present, writable 2 MiB page at physical 0,
   loads `CR3` with the PML4 address, sets `CR4.PAE` and `EFER.LME`, then sets
   `CR0.PG`.
5. A far jump to the 64-bit code selector switches to long mode; the 64-bit entry
   loads the data selectors, sets `RSP` to the boot stack, and calls
   `rust_main`.

No IDT is installed: a fault triple-faults, and QEMU runs with `-no-reboot` so the
gate sees a termination instead of a reboot loop. No page above the first 2 MiB is
mapped.

## 4. Serial contract (gate-asserted)

COM1 (`0x3F8`), 115200 8N1, FIFO enabled, no interrupts. The smoke kernel writes
these lines and nothing else before exiting:

```
[x86_64] long mode
[x86_64] serial ok
[x86_64] exit 0
```

The gate asserts `[x86_64] long mode` and `[x86_64] serial ok` in the serial log
and that the machine left through the exit device, not a timeout.

## 5. Exit contract

QEMU runs `-device isa-debug-exit,iobase=0x501,iosize=0x04`. A 32-bit write of
`v` to `0x501` makes QEMU exit with host status `(v << 1) | 1`:

| HAL call | value written | host status |
|---|---|---|
| `Exit::exit(0)` | `0x10` | `0x21` (33) — pass |
| `Exit::exit(non-zero)` | `0x11` | `0x23` (35) — failure |

A pass is therefore status 33 plus both markers. Status 124 (timeout) or anything
else is a failure.

## 6. Build and run

```sh
make -C x86-smoke preflight   # fails when x86_64-unknown-none is not installed
make -C x86-smoke image       # build/boot.img
make -C x86-smoke test        # boots, writes build/serial.log, accepts status 33
```

QEMU command (QEMU 8.2.2, device verified present on this host):

```sh
qemu-system-x86_64 -machine pc -m 128M -display none -serial stdio -no-reboot \
  -drive file=build/boot.img,format=raw,if=ide,index=0,media=disk \
  -device isa-debug-exit,iobase=0x501,iosize=0x04
```

The gate runs the compile-only check before the boot, so a compile error and a
boot failure are reported separately.

## 7. Why an own loader

D3 makes the x86_64 boot chain ours and D12 makes this the early validation of the
seam. `-kernel` with QEMU's multiboot loader would enter `_start` in 32-bit
protected mode with someone else's mode switch, so the smoke path would never
exercise the real-mode → protected-mode → long-mode transition we will own at
M5.4. The MBR plus payload above is the smallest chain that does.
