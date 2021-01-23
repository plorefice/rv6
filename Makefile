TARGET = riscv64gc-unknown-none-elf
OUTDIR = "target/$(TARGET)/debug"
RV6_ELF = "$(OUTDIR)/rv6"
RV6_BIN = "$(OUTDIR)/rv6.bin"

$(RV6_ELF):
	@cargo build

$(RV6_BIN): $(RV6_ELF)
	@riscv64-unknown-elf-objcopy -O binary "$<" "$@"

run: $(RV6_BIN)
	@qemu-system-riscv64 -M virt -m 256M -nographic -serial mon:stdio \
	     -bios ../opensbi/build/platform/generic/firmware/fw_jump.bin \
	     -kernel "$<"

clean:
	@cargo clean
