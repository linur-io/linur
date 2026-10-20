# Debugging linur

Scope: riscv64 under QEMU 8.2.2, the only target that boots (x86_64 is
compile-only until M5.4). Every claim below is either executed on this host at
M1.2, with its output shown, or marked **UNVERIFIED** with the reason. At M1.2
the kernel has a serial console through SBI, no tty (M3.1) and no display
device (M3.4), so the serial log is the only runtime evidence.

## 1. The serial console and the crash log

```sh
cd os
make run                      # interactive; initproc -> user_shell
make test                     # boots the suite (TEST defaults to 1), then exits
make test TEST=<app>          # boots one app as init (e.g. TEST=hello_world)
```

`make test` runs `timeout --foreground $(TEST_TIMEOUT) qemu-system-riscv64
$(QEMU_ARGS) </dev/null 2>&1 | tee last-serial.log` under `bash -o pipefail`,
so:

* the log is `os/last-serial.log`, covered by `os/last-*` in `.gitignore`
  (verified with `git check-ignore`); `SERIAL_LOG=<path>` overrides it;
* `tee` writes as bytes arrive, so the file holds everything printed up to a
  crash. During a suite run it was already 57 KB mid-run and 69 KB at the end;
  the panic block in §2 came out of the same pipeline;
* QEMU's status is the recipe's status: the harness echoes it as
  `[test] qemu exit code: 0`. A panic exits 255, `make` reports `Error 255`
  and exits 2. A clean suite exits 0 and the log ends with
  `[kernel] Idle process exit with exit_code 0 ...`.
* `make test` defaults to `TEST=1`; a command-line `TEST=<app>` replaces it and
  boots that program as init. `make run` keeps the interactive `initproc` shell,
  and `TEST_TIMEOUT` (default 300 s) bounds any run that never terminates, so a
  hung suite fails with `[test] TIMEOUT` instead of blocking CI.

`make run` stays interactive. Capture your own interactive session — serial
output only — with:

```sh
cd os && make run 2>&1 | tee my-session.log
```

Add `bash -o pipefail -c` if you also want QEMU's exit status: without it the
pipeline reports `tee`'s.

`-nographic` puts the serial console on stdio; the monitor shares it (§3).

### Stopping a run

The two run modes need different keys, and neither Ctrl-C nor Ctrl-D works in
both:

* `make test` — Ctrl-C stops it. QEMU runs under `timeout --foreground`, so it
  stays in the terminal's process group and the signal reaches it (verified: the
  run dies, make exits with SIGINT, no QEMU process is left). `TEST_TIMEOUT`
  caps a run that hangs even without a key. From another terminal,
  `pkill -f qemu-system-riscv64` always works.
* `make run [TEST=...]` — QEMU switches the terminal to raw mode (verified:
  `ICANON`, `ISIG` and `ECHO` are cleared while it runs), so **Ctrl-C and
  Ctrl-D are forwarded to the guest console, not to the host**: they do not stop
  the run, and a program that does not read the console (the whole suite) just
  ignores them. Quit with **Ctrl-A then lowercase `x`**; a bare `x` and
  uppercase `X` are not the escape (verified: only `x` exits). `Ctrl-A c`
  switches to the monitor, where `quit` exits too. If QEMU is killed hard
  (`SIGKILL`), the terminal stays raw — `stty sane` restores it. A terminal
  multiplexer that uses Ctrl-A as its prefix swallows the escape: send the
  prefix twice.
* Keys never help when stdin is not QEMU's stdin: `make test` hands QEMU
  `/dev/null`, so `Ctrl-A` sequences cannot arrive. That is why the harness
  needs a signal (`Ctrl-C`) rather than a guest keystroke.

## 2. Panic and stack trace

A kernel panic prints a location line and a backtrace block (real capture, a
deliberate panic in `rust_main`):

```
[ERROR] [kernel] panic at src/main.rs:57: debug-docs panic format test
---START BACKTRACE---
#0: ra=0x8020142a
#1: ra=0x80201d44
#2: ra=0x802010a4
#3: ra=0x80200010
---END   BACKTRACE---
```

* `[ERROR] ` is the log prefix from `os/src/logging.rs`;
  `[kernel] panic at <file>:<line>: <message>` is `os/src/lang_items.rs`
  formatting `PanicInfo::location()`, with `file` relative to `os/`.
* The frames are return addresses from the frame-pointer chain: a frame keeps
  the return address at `fp - 8` and the caller's `fp` at `fp - 16`, starting
  from `s0`. Rust omits frame pointers at `-O`, so the chain exists only
  because `os/.cargo/config.toml` passes `-Cforce-frame-pointers=yes`. The
  walk takes at most 16 frames and stops at `fp == 0`, a misaligned `fp`, a
  caller `fp` that does not advance, or a return address outside
  `[stext, etext)`: the kernel frame chain ends at the trap entry assembly,
  beyond which the saved values belong to the interrupted user context.
* Resolve the frames:

```sh
cd os && make symbolize                    # reads last-serial.log
make symbolize SERIAL_LOG=my-session.log   # any log with a backtrace block
```

`scripts/symbolize.sh` extracts every `#n: ra=0x...` line, subtracts one from
each address (a return address points past the call, so `ra` alone can land on
the next symbol) and runs `addr2line -f -C` on
`target/riscv64gc-unknown-none-elf/release/os`. No `-i`: inline chains make
one address resolve to several names. Verified output for the capture above
(paths abbreviated):

```
#0 0x8020142a __rustc::rust_begin_unwind
	.../os/src/lang_items.rs:28
#1 0x80201d44 core::panicking::panic_fmt
	/rustc/<hash>/library/core/src/panicking.rs:80
#2 0x802010a4 .Lpcrel_hi106
	.../os/src/main.rs:57
#3 0x80200010 .Lpcrel_hi0
	os.<hash>-cgu.09:?
```

* The resolver is binutils `addr2line` (2.42 here). Rustup's `llvm-tools` does
  **not** ship `llvm-addr2line`: the sysroot `bin/` has `llvm-objcopy`,
  `llvm-objdump`, `llvm-nm` and friends, but no addr2line, so a binutils
  install is a debugging dependency.
* Resolve against the ELF that produced the log. `make symbolize` uses the
  current build; for a log from an older build pass the ELF as the second
  script argument. A mismatch shows up as `??` or `__restore` frames.
* Odd frames are normal: `-f` prints whatever symbol covers the address, so
  `.L...` names are compiler-generated labels in assembly stubs, and `cgu:?`
  means the address has no line info. Inline chains are not expanded (`-i` is
  off): each frame yields exactly one function.
* Exit status: the handler ends in `sbi::shutdown(true)` (SRST shutdown with
  SystemFailure) and QEMU exits 255 (verified). A user program that exits
  non-zero reaches the same `shutdown(true)` through the idle process
  (`exit_current_and_run_next` in `os/src/task/mod.rs`), so a failed run is
  always a non-zero exit.

## 3. QEMU monitor

With `-nographic` the monitor and the serial console share stdio. The escape
keys, captured from QEMU's own `Ctrl-a h` output (`0x01 0x68` fed into a
`-nographic` QEMU's stdin; this sandbox has no terminal):

```
C-a h    print this help
C-a x    exit emulator
C-a s    save disk data back to file (if -snapshot)
C-a t    toggle console timestamps
C-a b    send break (magic sysrq)
C-a c    switch between console and monitor
C-a C-a  sends C-a
```

`Ctrl-a c` and `Ctrl-a x` were exercised the same way: the monitor banner
appeared after `Ctrl-a c`, and `Ctrl-a x` ended the run with `QEMU:
Terminated` — but no key was pressed on a real terminal. Without a terminal
(CI, scripts) skip the mux and use
`-monitor stdio -serial file:<log> -display none` instead.

Commands, verified against QEMU 8.2.2 with the CPU stopped:

```
(qemu) stop
(qemu) info registers
CPU#0
 pc       0000000080002d40
 mstatus  0000000a00000880
 satp     8000000000080433
 ...
(qemu) c                 # continue
```

* `info registers` — full CPU state: `pc`, `mstatus`, `satp`, all integer and
  FP registers. The `pc` is wherever the CPU happened to be; it is often
  inside RustSBI (`0x8000_xxxx`) because the idle kernel sits in an SBI call.
* `info mtree` — the memory map. The RAM line reads
  `0000000080000000-0000000087ffffff (prio 0, ram): riscv_virt_board.ram`.
* `x/16i 0x80200000` — disassemble. `$pc` is **not** accepted: `x/16i $pc` and
  `p $pc` answer `unknown register` (this monitor takes plain addresses), so
  copy `pc` out of `info registers` and paste the address.
* `x/4gx 0x80200000` (virtual, through `satp`) and `xp/4gx 0x80200000`
  (physical) both print the image — `help x` says "virtual memory dump",
  `help xp` says "physical memory dump" — because kernel mappings are identity
  (`MapType::Identical` in `os/src/mm/memory_set.rs`). Use `xp` when the CPU is
  stopped in user code, where `satp` does not map kernel memory.
* `help`, `help x`, `help info` list the rest; `info mem` gives the active
  virtual mappings, `info qtree` the device tree.
* `screendump` is not evidence of anything yet: with `-display none` and no
  display device in `QEMU_ARGS` it writes a blank 640x384 PPM (3068 non-zero
  bytes out of 737280). It becomes useful with the VGA console (M3.4).

## 4. gdb

The QEMU half is verified without gdb: `-s -S` listens on 127.0.0.1:1234 and
answers the remote protocol (`qSupported` →
`PacketSize=1000;qXfer:features:read+;vContSupported+;...`, `?` →
`T05thread:p01.01;`), with the CPU halted at reset.

The client half is **UNVERIFIED on this host**: no riscv-capable gdb is
installed.

* `/usr/bin/gdb` is GNU gdb 15.0.50 and rejects `set architecture riscv:rv64`
  with `Undefined item: "riscv:rv64"`. It reads the ELF
  (`file type elf64-little`, entry `0x80200000`) but cannot debug it.
* `gdb-multiarch`, `riscv64-unknown-elf-gdb` and `riscv64-linux-gnu-gdb` are
  all absent (`command -v`).

You need a riscv-capable gdb, e.g. the riscv64-unknown-elf toolchain's, or
Debian/Ubuntu's `gdb-multiarch` (unverified on this host).

The Makefile targets are **UNVERIFIED here**: the client side hardcodes
`riscv64-unknown-elf-gdb`, which is not installed (`make gdbserver` alone is
just QEMU and works, see the invocation above):

```sh
cd os && make gdbserver    # QEMU with -s -S, paused at reset
cd os && make gdbclient    # client in a second terminal
```

`make debug` runs the two in tmux (`/usr/bin/tmux` is installed) and needs
the same gdb. Manual equivalent:

```sh
qemu-system-riscv64 <QEMU_ARGS> -s -S
riscv64-unknown-elf-gdb -ex 'file target/riscv64gc-unknown-none-elf/release/os' \
    -ex 'set arch riscv:rv64' \
    -ex 'target remote localhost:1234'
```

Then, in gdb (**UNVERIFIED**):

```
(gdb) break rust_main
(gdb) continue
(gdb) info registers
(gdb) x/10i $pc
(gdb) bt
```

`bt` walks the same frame-pointer chain as the kernel backtrace, so it is only
as good as `-Cforce-frame-pointers=yes`.

## 5. Read the crash in this order

1. `tail -40 os/last-serial.log` — last words; the log survived the crash.
2. The `[kernel] panic at <file>:<line>: <message>` line — the source location.
3. `cd os && make symbolize` — every `ra=` frame becomes function + file:line.
4. `make gdbserver`, then `Ctrl-a c` (or `-monitor stdio`): `info registers`
   for `pc`/`satp`/`scause`, `x/16i <pc>`, `info mem`.
5. gdb: `break <function>`, `continue`, `bt` — needs a riscv-capable gdb
   (**UNVERIFIED** here).
