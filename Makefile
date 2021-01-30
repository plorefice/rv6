# Build target as recognized by Rust
TARGET = riscv64gc-unknown-none-elf

# Build artifacts
OUTDIR = target/$(TARGET)/debug
OPENSBI_BIN = opensbi/build/platform/generic/firmware/fw_jump.bin
RV6_STATICLIB = $(OUTDIR)/librv6.a
RV6_DYLIB = $(OUTDIR)/rv6.o
RV6_BIN = $(OUTDIR)/rv6

# Tools and utilities
CROSS_COMPILE ?= riscv64-unknown-elf-
OBJCOPY = $(CROSS_COMPILE)objcopy
LD = $(CROSS_COMPILE)ld
QEMU = qemu-system-riscv64 -M virt -m 256M -nographic -serial mon:stdio \
	-bios $(OPENSBI_BIN) -kernel

.PHONY = run test clean FORCE

$(RV6_STATICLIB): FORCE
	@cargo build

$(RV6_DYLIB): $(RV6_STATICLIB)
	@$(LD) -T linkers/riscv.ld -o "$@" --whole-archive "$<"

$(RV6_BIN): $(RV6_DYLIB)
	@$(OBJCOPY) -O binary "$<" "$@"

$(OPENSBI_BIN):
	@make -C opensbi CROSS_COMPILE=riscv64-unknown-elf- PLATFORM=generic

run: $(RV6_BIN) $(OPENSBI_BIN)
	@$(QEMU) "$<"

clean:
	@cargo clean

FORCE:
