# Build target as recognized by Rust
TARGET = riscv64gc-unknown-none-elf

# Build artifacts
OUTDIR = target/$(TARGET)/debug
OPENSBI_BIN = opensbi/build/platform/generic/firmware/fw_jump.bin
RV6_STATICLIB = $(OUTDIR)/librv6.a
RV6_DYLIB = rv6
RV6_BIN = rv6.bin

# Tools and utilities
CROSS_COMPILE ?= riscv64-elf-
OBJCOPY = $(CROSS_COMPILE)objcopy
LD = $(CROSS_COMPILE)ld
QEMU = qemu-system-riscv64 -M virt -cpu rv64,sv57=off -m 256M -nographic -serial mon:stdio \
	-bios $(OPENSBI_BIN) -kernel

$(RV6_STATICLIB): FORCE
	@cargo build --target $(TARGET) --manifest-path kernel/Cargo.toml

$(RV6_DYLIB): $(RV6_STATICLIB) ksymsgen
	@CROSS_COMPILE=$(CROSS_COMPILE) script/link-rv6.sh "$<"

$(RV6_BIN): $(RV6_DYLIB)
	@$(OBJCOPY) -O binary "$<" "$@"

$(OPENSBI_BIN):
	@make -C opensbi CROSS_COMPILE=$(CROSS_COMPILE) PLATFORM=generic

ksymsgen: FORCE
	@cargo build --manifest-path ksymsgen/Cargo.toml

qemu: $(RV6_BIN) $(OPENSBI_BIN)
	@$(QEMU) "$<"

clean:
	@cargo clean
	@rm -f "$(RV6_BIN)" "$(RV6_DYLIB)"

FORCE:

.PHONY = ksymsgen qemu test clean FORCE
