# Building
TARGET := riscv64gc-unknown-none-elf
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os
KERNEL_BIN := $(KERNEL_ELF).bin
DISASM_TMP := target/$(TARGET)/$(MODE)/asm
FS_IMG := ../user/build/fs.img
APPS := ../user/src/bin/*

# BOARD
BOARD := qemu
SBI ?= rustsbi
BOOTLOADER := ../bootloader/$(SBI)-$(BOARD).bin

# Building mode argument
ifeq ($(MODE), release)
	MODE_ARG := --release
endif

# KERNEL ENTRY
KERNEL_ENTRY_PA := 0x80200000

# Binutils straight from rustup's llvm-tools component; cargo-binutils is not needed.
SYSROOT := $(shell rustc --print sysroot)
HOST := $(shell rustc -vV | sed -n 's/^host: //p')
LLVM_BIN := $(SYSROOT)/lib/rustlib/$(HOST)/bin
OBJDUMP := $(LLVM_BIN)/llvm-objdump --arch-name=riscv64
OBJCOPY := $(LLVM_BIN)/llvm-objcopy --binary-architecture=riscv64

# Disassembly
DISASM ?= -x

# Run usertests or usershell
TEST ?=

build: preflight $(KERNEL_BIN) fs-img

# Check-only preflight: never downloads or mutates the toolchain.
preflight:
	@missing=""; \
	if [ ! -d "$(SYSROOT)/lib/rustlib/$(TARGET)/lib" ]; then missing="$$missing\n  rustup target add $(TARGET)"; fi; \
	if [ ! -d "$(SYSROOT)/lib/rustlib/src/rust/library" ]; then missing="$$missing\n  rustup component add rust-src"; fi; \
	if [ ! -x "$(LLVM_BIN)/llvm-objcopy" ] || [ ! -x "$(LLVM_BIN)/llvm-objdump" ]; then missing="$$missing\n  rustup component add llvm-tools-preview"; fi; \
	if [ -n "$$missing" ]; then \
		printf 'error: Rust toolchain is missing pieces (make never downloads anything):%b\n' "$$missing" >&2; \
		echo "run the commands above once, then re-run make." >&2; \
		exit 1; \
	fi

$(KERNEL_BIN): kernel
	@$(OBJCOPY) $(KERNEL_ELF) --strip-all -O binary $@

# Pack fs.img from user/build/stage, never from user/target: cargo hardlinks
# target/<triple>/release/<bin> into its deps cache, so writing an app there
# corrupts the cache for every later build.
fs-img: $(APPS)
	@cd ../user && make build TEST=$(TEST)
	@rm -f $(FS_IMG)
	@cd ../easy-fs-fuse && cargo run --release -- -s ../user/build/stage -t ../user/build/

$(APPS):

kernel:
	@echo Platform: $(BOARD)
	@cp src/linker-$(BOARD).ld src/linker.ld
	@cargo build --release
	@rm src/linker.ld

clean:
	@cargo clean

disasm: kernel
	@$(OBJDUMP) $(DISASM) $(KERNEL_ELF) | less

disasm-vim: kernel
	@$(OBJDUMP) $(DISASM) $(KERNEL_ELF) > $(DISASM_TMP)
	@vim $(DISASM_TMP)
	@rm $(DISASM_TMP)

run: run-inner

QEMU_ARGS := -machine virt \
			 -nographic \
			 -bios $(BOOTLOADER) \
			 -device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_PA) \
			 -drive file=$(FS_IMG),if=none,format=raw,id=x0 \
			 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0

QEMU_NAME := qemu-system-riscv64
qemu-version-check:
	@sh scripts/qemu-ver-check.sh $(QEMU_NAME)

run-inner: qemu-version-check build
	@qemu-system-riscv64 $(QEMU_ARGS)

debug: qemu-version-check build
	@tmux new-session -d \
		"qemu-system-riscv64 $(QEMU_ARGS) -s -S" && \
		tmux split-window -h "riscv64-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

gdbserver: qemu-version-check build
	@qemu-system-riscv64 $(QEMU_ARGS) -s -S

gdbclient:
	@riscv64-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'

.PHONY: build preflight kernel clean disasm disasm-vim run-inner fs-img gdbserver gdbclient qemu-version-check
