# SPDX-License-Identifier: GPL-3.0-only
# x86_64 smoke path: build the 33-sector boot image and boot it under QEMU.

TARGET := x86_64-unknown-none
MODE := release
IMAGE := build/boot.img
SERIAL_LOG := build/serial.log
IMAGE_SIZE := 16896
PAYLOAD_SIZE := 16384

SYSROOT := $(shell rustc --print sysroot)
HOST := $(shell rustc -vV | sed -n 's/^host: //p')
LLVM_BIN := $(SYSROOT)/lib/rustlib/$(HOST)/bin
OBJCOPY := $(LLVM_BIN)/llvm-objcopy

# Hard cap for make test, in seconds: a hung boot must fail, not hang CI.
TEST_TIMEOUT ?= 60

QEMU := qemu-system-x86_64
QEMU_ARGS := -machine pc -m 128M -display none -serial stdio -no-reboot \
	-drive file=$(IMAGE),format=raw,if=ide,index=0,media=disk \
	-device isa-debug-exit,iobase=0x501,iosize=0x04

.PHONY: all preflight image test clean

all: image

# Check-only preflight: never downloads or mutates the toolchain.
preflight:
	@missing=""; \
	if [ ! -d "$(SYSROOT)/lib/rustlib/$(TARGET)/lib" ]; then missing="$$missing\n  rustup target add $(TARGET)"; fi; \
	if [ ! -x "$(OBJCOPY)" ]; then missing="$$missing\n  rustup component add llvm-tools-preview"; fi; \
	if [ -n "$$missing" ]; then \
		printf 'error: Rust toolchain is missing pieces (make never downloads anything):%b\n' "$$missing" >&2; \
		echo "run the commands above once, then re-run make." >&2; \
		exit 1; \
	fi

image: preflight
	@cargo build --release
	@mkdir -p build
	@$(OBJCOPY) -O binary target/$(TARGET)/$(MODE)/x86-smoke $(IMAGE)
	@size=$$(stat -c %s $(IMAGE)); \
	if [ $$size -gt $(IMAGE_SIZE) ]; then \
		echo "error: payload is $$((size - 512)) bytes, the limit is $(PAYLOAD_SIZE)" >&2; \
		rm -f $(IMAGE); \
		exit 1; \
	fi; \
	truncate -s $(IMAGE_SIZE) $(IMAGE); \
	echo "[image] $(IMAGE) $$(stat -c %s $(IMAGE)) bytes"

test: image
	@echo "serial log: $(SERIAL_LOG)"
	@bash -o pipefail -c 'timeout --foreground $(TEST_TIMEOUT) $(QEMU) $(QEMU_ARGS) </dev/null >$(SERIAL_LOG) 2>&1; \
		status=$$?; \
		echo $$status > $(SERIAL_LOG).status; \
		if [ $$status -eq 124 ]; then echo "[test] TIMEOUT after $(TEST_TIMEOUT)s"; \
		else echo "[test] qemu exit code: $$status"; fi; \
		[ $$status -eq 33 ]'

clean:
	@cargo clean
	@rm -rf build
