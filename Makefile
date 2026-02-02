# Build target as recognized by Rust
TARGET = riscv64gc-lp64d

# Build artifact
RV6_STATICLIB = kernel/target/$(TARGET)/debug/librv6.a

# Output files
OUTPUT_DIR = output
RV6_DYLIB = $(OUTPUT_DIR)/rv6
RV6_BIN = $(OUTPUT_DIR)/rv6.bin

# Tools and utilities
CROSS_COMPILE ?= riscv64-elf-
OBJCOPY = $(CROSS_COMPILE)objcopy
LD = $(CROSS_COMPILE)ld
QEMU = qemu-system-riscv64 -M virt -cpu rv64,sv39=on -m 256M -nographic -serial mon:stdio \
		-device virtio-blk-device,serial=rv6-blk-dev,drive=hd0 -drive file=hdd.img,format=raw,id=hd0,if=none

all: $(RV6_BIN)

$(RV6_STATICLIB): FORCE
	@cd kernel && cargo build

$(RV6_DYLIB): $(RV6_STATICLIB) ksymsgen
	@mkdir -p $(OUTPUT_DIR)
	@CROSS_COMPILE=$(CROSS_COMPILE) script/link-rv6.sh kernel/src/arch/riscv/linker/qemu.ld "$@" "$<"

$(RV6_BIN): $(RV6_DYLIB)
	@$(OBJCOPY) -O binary "$<" "$@"

ksymsgen: FORCE
	@cd ksymsgen && cargo build

qemu: $(RV6_BIN)
	@$(QEMU) -kernel "$<"

debug: $(RV6_BIN)
	@$(QEMU) -kernel "$<" -S -s

clean:
	@cargo clean
	@cd kernel && cargo clean
	@rm -rf "$(OUTPUT_DIR)"

FORCE:

.PHONY = all ksymsgen qemu debug test clean FORCE
