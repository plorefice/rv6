# Build target as recognized by Rust
TARGET = riscv64gc-unknown-none-elf

# Build artifacts
OUTDIR = target/$(TARGET)/debug
RV6_ELF = $(OUTDIR)/rv6
RV6_BIN = $(OUTDIR)/rv6.bin
OPENSBI_BIN = opensbi/build/platform/generic/firmware/fw_jump.bin
TEST_BIN = $(OUTDIR)/rv6-test.bin

# Tools and utilities
CROSS_COMPILE ?= riscv64-unknown-elf-
OBJCOPY = $(CROSS_COMPILE)objcopy
QEMU = qemu-system-riscv64 -M virt -m 256M -nographic -serial mon:stdio \
	-bios $(OPENSBI_BIN) -kernel

.PHONY = run test clean FORCE

$(RV6_ELF): FORCE
	@cargo build

$(RV6_BIN): $(RV6_ELF)
	@$(OBJCOPY) -O binary "$<" "$@"

$(OPENSBI_BIN):
	@make -C opensbi CROSS_COMPILE=riscv64-unknown-elf- PLATFORM=generic

run: $(RV6_BIN) $(OPENSBI_BIN)
	@$(QEMU) "$<"

test:
# HACK: extract the path to the generated test binary by setting the runner to "echo"
#      and assigning its stdout to the TEST_ELF variable.
	$(eval TEST_ELF=$(shell cd kernel && CARGO_TARGET_RISCV64GC_UNKNOWN_NONE_ELF_RUNNER="echo" cargo test))
	@$(OBJCOPY) -O binary "$(TEST_ELF)" "$(TEST_BIN)"
	@$(QEMU) "$(TEST_BIN)"

clean:
	@cargo clean

FORCE:
