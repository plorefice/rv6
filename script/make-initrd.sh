#!/bin/bash

# Build the initrd image for rv6.

set -euo pipefail

cd "$(dirname "$0")/.."

# Build init program
INIT_SRCDIR=userland/init
INIT_OUTDIR=output/userland/init

mkdir -p "$INIT_OUTDIR"
riscv64-elf-as "$INIT_SRCDIR/entry.S" -o "$INIT_OUTDIR/init.o"
riscv64-elf-ld -T "$INIT_SRCDIR/linker.ld" -o "$INIT_OUTDIR/init.elf" "$INIT_OUTDIR/init.o"
riscv64-elf-objcopy -O binary "$INIT_OUTDIR/init.elf" "$INIT_OUTDIR/init"

# Create initrd fs
mkdir -p output/initrd
mkdir -p output/initrd/bin
cp "$INIT_OUTDIR/init" output/initrd/bin/init

# Create cpio initrd image
cd output/initrd
find . -print0 | perl -0pe 's|^\./||' | cpio -0 -o --format=newc > ../initrd.cpio
