# ----------------------------
# Configuration
# ----------------------------

TARGET        := "riscv64gc-lp64d"
PROFILE       := "release"
OUTDIR        := "out"

CROSS_COMPILE := env_var_or_default("CROSS_COMPILE", "riscv64-elf-")
OBJCOPY       := CROSS_COMPILE + "objcopy"
GDB		      := CROSS_COMPILE + "gdb"

RV6_STATICLIB := "kernel/target/" + TARGET + "/" + PROFILE + "/librv6.a"
RV6_DYLIB     := OUTDIR + "/rv6"
RV6_BIN       := OUTDIR + "/rv6.bin"
INITRD        := OUTDIR + "/initrd.cpio"
HDDIMG        := OUTDIR + "/hdd.img"

QEMU             := "qemu-system-riscv64"
QEMU_ARGS_BASE   := "-M virt -cpu rv64,sv39=on -m 256M -device ramfb -serial mon:stdio"
QEMU_ARGS_INITRD := "-initrd " + INITRD
QEMU_ARGS_INPUT  := "-device virtio-keyboard-device"
QEMU_ARGS_DISK   := "-device virtio-blk-device,serial=rv6-blk-dev,drive=hd0 " + \
                    "-drive file=" + HDDIMG + ",format=raw,id=hd0,if=none"
QEMU_ARGS_LOGS   := "-d guest_errors -D " + OUTDIR + "/qemu.log"
QEMU_ARGS        := QEMU_ARGS_BASE + " " + QEMU_ARGS_INITRD + " " + QEMU_ARGS_INPUT + " " + \
					QEMU_ARGS_DISK + " " + QEMU_ARGS_LOGS

# Default target
default: run

# ----------------------------
# Kernel build
# ----------------------------

kernel:
	cd kernel && cargo build {{ if PROFILE == "release" { "--release" } else { "" } }}

kernel-lib: kernel
	@true  # marker target

kernel-elf: kernel-lib ksymsgen
	mkdir -p {{OUTDIR}}
	CROSS_COMPILE={{CROSS_COMPILE}} \
	  scripts/link-rv6.sh \
	    kernel/src/arch/riscv/linker/qemu.ld \
	    {{RV6_DYLIB}} \
	    {{RV6_STATICLIB}}

kernel-bin: kernel-elf
	{{OBJCOPY}} -O binary {{RV6_DYLIB}} {{RV6_BIN}}

# ----------------------------
# Host tools
# ----------------------------

ksymsgen:
	cargo build -p ksymsgen

# ----------------------------
# Userland build
# ----------------------------

userland:
	cd userland && cargo build --release

# ----------------------------
# Initramfs and disk image
# ----------------------------

initrd: userland
	cd userland && ./install.sh
	cd out/rootfs && find . -print0 | perl -0pe 's|^\./||' | cpio -0 -o --format=newc > ../initrd.cpio

hddimg: initrd
	mkdir -p {{OUTDIR}}
	genext2fs -b 16384 -d out/rootfs {{HDDIMG}}

# ----------------------------
# QEMU
# ----------------------------

run: kernel-bin hddimg
	{{QEMU}} {{QEMU_ARGS}} -kernel {{RV6_BIN}}

debug: kernel-bin hddimg
	{{QEMU}} {{QEMU_ARGS}} -kernel {{RV6_BIN}} -S -s

# ----------------------------
# Debugging and testing
# ----------------------------

gdb:
	{{GDB}} {{RV6_DYLIB}} -ex "target remote :1234"

# ----------------------------
# Utilities
# ----------------------------

lint:
	cargo clippy
	cd kernel && cargo clippy
	cd userland && cargo clippy

fmt:
	cargo fmt
	cd kernel && cargo fmt
	cd userland && cargo fmt

clean:
	cargo clean
	cd kernel && cargo clean
	cd userland && cargo clean
	rm -rf {{OUTDIR}}

help:
	just --list
