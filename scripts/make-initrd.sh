#!/bin/bash

# Build the initrd image for rv6.

set -euo pipefail

cd "$(dirname "$0")/.."

# Build init program
OUTDIR=out
INIT_SRCDIR=userland/init
INIT_OUTDIR=$OUTDIR/userland/init

mkdir -p "$INIT_OUTDIR"
riscv64-elf-as "$INIT_SRCDIR/entry.S" -o "$INIT_OUTDIR/init.o"
riscv64-elf-ld -T "$INIT_SRCDIR/linker.ld" -o "$INIT_OUTDIR/init.elf" "$INIT_OUTDIR/init.o"
riscv64-elf-objcopy -O binary "$INIT_OUTDIR/init.elf" "$INIT_OUTDIR/init"

# Create initrd fs
mkdir -p $OUTDIR/initrd
mkdir -p $OUTDIR/initrd/bin
cp "$INIT_OUTDIR/init" $OUTDIR/initrd/bin/init

# Create cpio initrd image
cd $OUTDIR/initrd
find . -print0 | perl -0pe 's|^\./||' | cpio -0 -o --format=newc > ../initrd.cpio
