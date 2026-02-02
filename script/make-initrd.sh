#!/bin/bash

# Build the initrd image for rv6.

set -euo pipefail

cd "$(dirname "$0")/.."

# Build init program
mkdir -p output/userland/init
riscv64-elf-as userland/init/entry.S -o output/userland/init/init
riscv64-elf-objcopy -O binary output/userland/init/init

# Create initrd fs
mkdir -p output/initrd
mkdir -p output/initrd/bin
cp output/userland/init/init output/initrd/bin/init

# Create cpio initrd image
cd output/initrd
find . -print0 | cpio -0 -o --format=newc > ../initrd.cpio
