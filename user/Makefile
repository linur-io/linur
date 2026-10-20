TARGET := riscv64gc-unknown-none-elf
MODE := release
APP_DIR := src/bin
TARGET_DIR := target/$(TARGET)/$(MODE)
APPS := $(wildcard $(APP_DIR)/*.rs)
ELFS := $(patsubst $(APP_DIR)/%.rs, $(TARGET_DIR)/%, $(APPS))
BINS := $(patsubst $(APP_DIR)/%.rs, $(TARGET_DIR)/%.bin, $(APPS))

# Stage layout for fs.img: build/stage/ holds one plain copy of every app ELF,
# named as it must appear in the image. $(TARGET_DIR)/<bin> is hardlinked into
# cargo's deps cache, so nothing is ever written there after the build.
# $(FS_IMG) lives in build/, outside the stage dir, so it cannot pack itself.
STAGE_DIR := build/stage
FS_IMG := build/fs.img

# Binutils straight from rustup's llvm-tools component; cargo-binutils is not needed.
SYSROOT := $(shell rustc --print sysroot)
HOST := $(shell rustc -vV | sed -n 's/^host: //p')
LLVM_BIN := $(SYSROOT)/lib/rustlib/$(HOST)/bin
OBJDUMP := $(LLVM_BIN)/llvm-objdump --arch-name=riscv64
OBJCOPY := $(LLVM_BIN)/llvm-objcopy --binary-architecture=riscv64
CP := cp

TEST ?=

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

elf: preflight $(APPS)
	@cargo build --release

stage: elf
	@rm -rf $(STAGE_DIR)
	@mkdir -p $(STAGE_DIR)
	@set -e; for elf in $(ELFS); do $(CP) $$elf $(STAGE_DIR)/; done
	@if [ "$(TEST)" = "1" ]; then $(CP) $(STAGE_DIR)/usertests $(STAGE_DIR)/initproc; fi

binary: elf
	@$(foreach elf, $(ELFS), $(OBJCOPY) $(elf) --strip-all -O binary $(patsubst $(TARGET_DIR)/%, $(TARGET_DIR)/%.bin, $(elf));)

build: binary stage

clean:
	@cargo clean
	@rm -rf build

.PHONY: preflight elf stage binary build clean
