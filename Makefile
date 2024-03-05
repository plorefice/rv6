# Build target as recognized by Rust
TARGET = riscv64gc-unknown-none-elf

# Build artifacts
OUTDIR = target/$(TARGET)/debug
RV6_STATICLIB = $(OUTDIR)/librv6.a
RV6_DYLIB = rv6
RV6_BIN = rv6.bin

# Tools and utilities
CROSS_COMPILE ?= riscv64-elf-
OBJCOPY = $(CROSS_COMPILE)objcopy
LD = $(CROSS_COMPILE)ld
QEMU = qemu-system-riscv64 -M virt -cpu rv64,sv39=on -m 256M -nographic -serial mon:stdio

# Default to QEMU when no platform is selected
ifeq ($(PLATFORM),)
PLATFORM = qemu
endif

all: $(RV6_BIN)

$(RV6_STATICLIB): FORCE
	@cargo build --target $(TARGET) --manifest-path kernel/Cargo.toml --features config-${PLATFORM}

$(RV6_DYLIB): $(RV6_STATICLIB) ksymsgen
	@CROSS_COMPILE=$(CROSS_COMPILE) script/link-rv6.sh kernel/src/arch/riscv/linker/${PLATFORM}.ld "$<"

$(RV6_BIN): $(RV6_DYLIB)
	@$(OBJCOPY) -O binary "$<" "$@"

ksymsgen: FORCE
	@cargo build --manifest-path ksymsgen/Cargo.toml

qemu: $(RV6_BIN)
	@$(QEMU) "$<"

debug: $(RV6_BIN)
	@$(QEMU) -kernel "$<" -S -s

clean:
	@cargo clean
	@rm -f "$(RV6_BIN)" "$(RV6_DYLIB)"

FORCE:

.PHONY = all ksymsgen qemu debug test clean FORCE
