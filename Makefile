# Build target as recognized by Rust
TARGET = riscv64gc-lp64d

# Build artifact
RV6_STATICLIB = kernel/target/$(TARGET)/debug/librv6.a

# Output files
OUTDIR = out
RV6_DYLIB = $(OUTDIR)/rv6
RV6_BIN = $(OUTDIR)/rv6.bin
INITRD = $(OUTDIR)/initrd.cpio
HDDIMG = $(OUTDIR)/hdd.img

# Tools and utilities
CROSS_COMPILE ?= riscv64-elf-
OBJCOPY = $(CROSS_COMPILE)objcopy
LD = $(CROSS_COMPILE)ld
QEMU = qemu-system-riscv64 -M virt -cpu rv64,sv39=on -m 256M -nographic -serial mon:stdio -initrd $(INITRD) \
		-device virtio-blk-device,serial=rv6-blk-dev,drive=hd0 -drive file=$(HDDIMG),format=raw,id=hd0,if=none

all: $(RV6_BIN)

$(RV6_STATICLIB): FORCE
	@cd kernel && cargo build

$(RV6_DYLIB): $(RV6_STATICLIB) ksymsgen
	@mkdir -p $(OUTDIR)
	@CROSS_COMPILE=$(CROSS_COMPILE) scripts/link-rv6.sh kernel/src/arch/riscv/linker/qemu.ld "$@" "$<"

$(RV6_BIN): $(RV6_DYLIB)
	@$(OBJCOPY) -O binary "$<" "$@"

ksymsgen: FORCE
	@cargo build -p ksymsgen

$(INITRD): FORCE
	@scripts/make-initrd.sh

$(HDDIMG): FORCE
	@mkdir -p $(OUTDIR)
	@dd if=/dev/zero of="$@" bs=1M count=64
	@mkfs.ext2 -F "$@"

qemu: $(RV6_BIN) $(INITRD) $(HDDIMG)
	@$(QEMU) -kernel "$<"

debug: $(RV6_BIN) $(INITRD)
	@$(QEMU) -kernel "$<" -S -s

clean:
	@cargo clean
	@cd kernel && cargo clean
	@rm -rf "$(OUTDIR)"

FORCE:

.PHONY = all ksymsgen qemu debug test clean FORCE
