# Build target as recognized by Rust
TARGET = riscv64gc-unknown-none-elf

# Build artifacts
RV6_STATICLIB = kernel/target/$(TARGET)/debug/librv6.a
RV6_DYLIB = rv6
RV6_BIN = rv6.bin

# Tools and utilities
CROSS_COMPILE ?= riscv64-elf-
OBJCOPY = $(CROSS_COMPILE)objcopy
LD = $(CROSS_COMPILE)ld
QEMU = qemu-system-riscv64 -M virt -cpu rv64,sv39=on -m 256M -nographic -serial mon:stdio \
		-device virtio-blk-device,drive=hd0 -drive file=hdd.img,format=raw,id=hd0

all: $(RV6_BIN)

$(RV6_STATICLIB): FORCE
	@cd kernel && cargo build

$(RV6_DYLIB): $(RV6_STATICLIB) ksymsgen
	@CROSS_COMPILE=$(CROSS_COMPILE) script/link-rv6.sh kernel/src/arch/riscv/linker/qemu.ld "$<"

$(RV6_BIN): $(RV6_DYLIB)
	@$(OBJCOPY) -O binary "$<" "$@"

ksymsgen: FORCE
	@cd ksymsgen && cargo build

qemu: $(RV6_BIN)
	@$(QEMU) -kernel "$<"

debug: $(RV6_BIN)
	@$(QEMU) -kernel "$<" -S -s

clean:
	@cd kernel && cargo clean
	@rm -f "$(RV6_BIN)" "$(RV6_DYLIB)" rv6.map

FORCE:

.PHONY = all ksymsgen qemu debug test clean FORCE
