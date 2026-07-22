# AGENTS.md

Guidance for AI coding agents working in the **rv6** repository.

## Project overview

rv6 is an educational, Unix-like **RISC-V kernel** inspired by
[xv6](https://pdos.csail.mit.edu/6.828/2020/xv6.html) and the Linux kernel. It is
`#![no_std]`, boots on the QEMU `virt` machine via OpenSBI, and is being extended toward a
userland (init program, syscalls, fork/wait, virtio-blk, ext2 rootfs, initrd fallback).
Not tested on real hardware.

License: dual MIT / Apache-2.0.

## Repository layout

This repo contains **three separate Cargo projects**. Run `cargo` from the correct directory.

| Path | Cargo project | Contents |
|------|---------------|----------|
| `Cargo.toml` (root) | workspace: `crates/*`, `tools/*` (excludes `kernel`, `userland`) | shared libs + host tools |
| `kernel/` | standalone (`rv6`, produces `lib` + `staticlib`) | the kernel |
| `userland/` | workspace: `runtime`, `apps/*` | userspace runtime + init |
| `crates/` | `cpio`, `elf`, `fdt`, `ext2` | `no_std` support libraries |
| `tools/ksymsgen/` | host tool | generates kallsyms-style symbol table |
| `scripts/` | — | `link-rv6.sh` (link ELF), `make-initrd.sh` |
| `out/` | — | build artifacts (gitignored) |

Kernel subsystems live under `kernel/src/`:

- `arch/hal/` — trait-based abstraction between generic and arch-specific code
- `arch/riscv/` — RISC-V implementation (MMU Sv39/Sv48, traps, SBI, entry, context switch)
- `drivers/` — FDT-driven drivers (PLIC, virtio-mmio, NS16550 UART, syscon)
- `mm/` — addresses, allocators (bump/bitmap), DMA, MMIO mapping
- `proc/` — process table, round-robin scheduler, ELF process loading
- `block.rs` — block device table + `BlockDevCursor` (byte cursor over virtio-blk for ext2)
- `syscall.rs`, `initrd.rs`, `ksyms.rs`, `panic.rs`

## Build & run

**Use `just`, not `make`.** There is no Makefile; the `README.md` build section is stale.

```bash
just --list        # list all targets
just initrd        # build userland, install to out/rootfs, pack out/initrd.cpio
just hddimg        # pack out/rootfs into out/hdd.img (ext2 via genext2fs)
just run           # build kernel + launch QEMU (default target)
just debug         # QEMU waits for GDB on :1234 (-S -s)
just gdb           # connect GDB to a running debug session
just clean         # clean all three projects + out/
```

Other targets: `just kernel` (staticlib), `just kernel-elf` (linked `out/rv6`),
`just kernel-bin` (`out/rv6.bin`), `just userland`, `just ksymsgen`.

Note: `just run` only depends on `kernel-bin`. It does **not** rebuild the initrd, disk
image, or userland. After `just clean` or a fresh clone, run `just initrd` then
`just hddimg` before `just run` — initrd populates `out/rootfs`, which hddimg packs.

At boot the kernel mounts the virtio-blk ext2 image as rootfs and loads `/init` from it,
falling back to the CPIO initrd if that fails.

### Prerequisites

- Rust **nightly** (pinned via `rust-toolchain.toml`) with `rust-src`, `rustfmt`, `clippy`
- RISC-V cross toolchain, prefix `riscv64-elf-` (gcc/ld/ar/objcopy/gdb).
  Override with `CROSS_COMPILE=riscv64-unknown-elf- just kernel-elf`.
- `qemu-system-riscv64`, `just`, `rg` (ripgrep — required by `link-rv6.sh`), `cpio`, `perl`,
  `genext2fs` (for `just hddimg`; e.g. `brew install genext2fs` on macOS)

## Toolchain constraints

- **Nightly only.** Do not attempt to build on stable.
- Bare-metal build uses `build-std` + a custom JSON target (`riscv64gc-lp64d.json`,
  ABI `lp64d`, `panic = abort`), configured in `kernel/.cargo/config.toml` and
  `userland/.cargo/config.toml`.
- Rust **edition 2024** in all crates. Use `#[unsafe(no_mangle)]`, not `#[no_mangle]`.
- Kernel features: `sv39` (default) and `sv48` are **mutually exclusive**.
- `kernel/build.rs` compiles `.S` files with `riscv64-elf-gcc -march=rv64gc -mabi=lp64d`.

## Testing

Host-side tests run in the root workspace:

```bash
cargo test -p cpio
cargo test -p fdt     # uses crates/fdt/tests/data/qemu-riscv.dtb
cargo test -p ext2 --features std   # uses crates/ext2/tests/data/ext2.img
```

There is no custom bare-metal test runner wired up, and no CI. Some `#[cfg(test)]` unit tests
exist in the kernel (e.g. `kernel/src/mm/allocator/bitmap.rs`) but are not run by `just`.

## Conventions

Follow the lints declared in `kernel/src/lib.rs` — they are the de facto style guide:

```rust
#![no_std]
#![warn(missing_docs)]
#![warn(clippy::missing_safety_doc)]
#![warn(clippy::undocumented_unsafe_blocks)]
#![deny(unsafe_op_in_unsafe_fn)]
```

- Document every public item; add `//!` module docs.
- Every `unsafe fn` needs a `/// # Safety` section; every `unsafe { }` block needs a
  `// SAFETY:` comment.
- Arch-specific code goes in `kernel/src/arch/riscv/`; generic code interacts with it only
  through `arch::hal` traits.
- Logging: use the macros in `kernel/src/macros.rs` (`kprintln!`, `kprint!`, `kdbg!`, etc.),
  not `println!`.
- Synchronization: use primitives under `sync`.
- Syscalls return `Result<T, Errno>` with a negative-errno convention.
- No formatting/clippy config files exist — use defaults: `cargo fmt --all` and
  `cargo clippy` (per project).

## Gotchas

- **Keep syscall numbers in sync** between `kernel/src/syscall.rs` and
  `userland/runtime/src/syscall.rs`.
- The init binary must be installed as `out/rootfs/init` (`userland/install.sh`). That path
  is packed into both `out/initrd.cpio` (`just initrd`) and the ext2 rootfs image
  `out/hdd.img` (`just hddimg` via `genext2fs -d out/rootfs`).
- rust-analyzer is configured (`.vscode/settings.json`) to link all three Cargo projects.
- The `README.md` references a Makefile and an older crate layout (`kmm/`, `riscv/`, `sbi/`)
  that no longer exist — trust `justfile` and the current tree instead.
