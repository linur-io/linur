# linur — the architecture seam (`hal`)

Status: interface contract for M1.5. Read with `spec/02` (M1.5 deliverable) and
D12. M1.5 covers exactly two primitives — console output and machine exit — because
they are the only things the riscv64 kernel and the x86_64 smoke path both need
before anything else exists. The rest of the seam is M5.3's work; adding a
capability here before a second implementation needs it is out of scope.

## 1. The crate

`hal/` is a dependency-free `no_std` crate. It defines the traits only; each
machine keeps its implementation next to the hardware it wraps, so the crate stays
platform-free and both kernels can build against it.

```rust
/// A byte sink for kernel messages.
pub trait Console {
    /// Write one byte. No encoding is assumed: the caller may forward any byte.
    fn put_byte(&self, byte: u8);
}

/// The machine's stop primitive.
pub trait Exit {
    /// Stop the machine. `0` means success; any other value means failure.
    /// A correct implementation does not return.
    fn exit(&self, code: i32) -> !;
}
```

The traits are object-safe and allocation-free on purpose: an implementation must
be usable from the first instruction after the platform's entry point, before any
heap, page table, trap handler or lock exists.

## 2. Implementations

| implementation | machine | console | exit |
|---|---|---|---|
| `os::console::Stdout` | riscv64, QEMU `virt` | SBI legacy console (ecall) | — |
| `os::board` (the qemu board type) | riscv64, QEMU `virt` | — | SiFive test finisher at `0x0010_0000` |
| `x86_smoke::Uart` | x86_64, QEMU `pc` | 16550 UART at COM1 `0x3F8` | — |
| `x86_smoke::DebugExit` | x86_64, QEMU `pc` | — | `isa-debug-exit` at `0x501` |

Both kernels route their real console output and their real exit through these
traits; they are not parallel implementations kept for the test's sake.

## 3. Invariants

* `Console::put_byte` accepts any byte value; the riscv64 SBI call and the 16550
  both take a raw byte, and `os/src/console.rs` forwards non-UTF-8 bytes
  unchanged.
* `Exit::exit` never returns. A machine that cannot stop is a bug, not an error
  path.
* Exit-code mapping: `0` is success, non-zero is failure. The riscv64 test device
  encodes the code itself; `isa-debug-exit` has no code channel, so the x86_64
  implementation collapses failure to one value and reports success as host status
  33 (`docs/x86_64-smoke.md` §5).
* Neither trait allocates, locks or panics. Neither trait needs interrupts enabled.

## 4. Deliberately not in this seam yet

Memory and paging, traps and context switch, time, device dispatch, SMP. They are
M5.3's subject, when both ports have a real implementation to generalise over. The
M1.5 check that keeps this honest is the compile-only x86_64 build of `hal` and
`x86-smoke` in the machine gate (`spec/02` M1.5).
