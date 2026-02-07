#!/bin/bash

# This script is used to install the userland applications into the initramfs.

set -euo pipefail

# Configure the output directory and build profile
OUTDIR=../out/rootfs
PROFILE=release

# Create the output directory if it doesn't exist
mkdir -p $OUTDIR

# Install the applications
install -v -m 755 target/riscv64gc-lp64d/$PROFILE/init $OUTDIR/init
