# rv6

> An educational kernel with a focus on the RISC-V architecture.
> Inspired by [xv6](https://pdos.csail.mit.edu/6.828/2020/xv6.html) and the Linux kernel.

:warning: **The project is still very much a WIP and it will probably remain so** :warning:

Given the scope of the project and my free time, I'd be very surprised to actually get to a working
userspace init program anytime soon.

## Components

The project is split into many modular crates, in the hopes of confining as much arch-specific
code as possible into dedicated modules.

The followings are all `no_std` crates belonging to the root workspace, which are used to put
together the final OS:

- [`kernel`](kernel/) contains the core of _rv6_ and and produces the final kernel binary
- [`kmm`](kmm/), short for _kernel memory management_, is a collection of arch-generic memory
  management facilities (eg. allocators, memory traits, etc.)
- [`rvalloc`](rvalloc/) aims to be a generic fully-fledged kernel heap allocator
- [`riscv`](riscv/) contains all RISC-V specific and kernel-independent code, such as register
  definitions, wrappers around special instructions, memory abstraction, MMU support etc.
- [`sbi`](sbi/) is a client-side implementation of the RISC-V
  [Supervisor Binary Interface](https://github.com/riscv/riscv-sbi-doc/blob/master/riscv-sbi.adoc)
  for interacting with platform-specific runtime firmware (_SEE_).

Additionally, [`ksymsgen`](ksymsgen/) is a small command-line utility which parses the output of
`nm` to generate a code section containing all the kernel symbols to be used for symbol resolution
in kernel stack traces. It's basically Linux's
[kallsyms](https://elixir.bootlin.com/linux/latest/source/scripts/kallsyms.c) without all the
fanciness.

## Requirements

Aside from a working Rust installation ([rustup.rs](https://rustup.rs/) recommended), a bunch of
stuff is required to run _rv6_:

- Nigthly version of the Rust compiler, which as of now is the only way to build the kernel

  `$ rustup toolchain install nightly`

- Support for the `riscv64` target in Rust, obviously

  `$ rustup target add riscv64gc-unknown-none-elf`

- GCC toolchain for RISC-V cross-compilation, to build the (few) assembly files in the tree

  Getting this component is very distribution-dependent, but on Ubutun a simple
  `sudo apt install gcc-riscv64-unknown-elf` will suffice.

- GDB with support for the RISC-V architecture (trust me, you will need it :wink:)

  Again, on Ubuntu `apt` comes through for us: `sudo apt install gdb-multiarch`

- QEMU with support for RISC-V machines, to finally run it

  As above, on Ubuntu: `sudo apt install qemu-system-misc`

If something is missing from this list and you figure it out, please let me know by submitting a
PR.

## Checking out the code

Use the `--recursive` option when cloning the repository to automatically download its submodules.

Alternatively, run `git submodule update --init --recursive` after the clone.

## Building the kernel

Before getting started, make sure that you are either using the nightly toolchain system-wide, or
have set up an override for this project's directory with `rustup override`.

After that, building the project is a simple matter of running:

```bash
$ make rv6       # to produce an ELF executable, or...
$ make rv6.bin   # for a binary file which can be loaded by QEMU
```

## Running the kernel

The `qemu` Makefile target is provided for simplicity:

```bash
$ make qemu
```

This will spawn a QEMU machine which will first boot to OpenSBI, an open-source SBI-compatible
loader for RISC-V, which will in turn boot the kernel.

In order to make QEMU wait for a GDB remote connection before starting up, you can use the `debug`
target instead.

## Roadmap

- [x] Bootloader hand-off
- [x] Early relocation and paging setup (v1)
- [x] Early console (v1)
- [x] Heap allocator
- [ ] FDT parsing
- [ ] Syscall interface
- [ ] Idle kernel task
- [ ] Filesystem support
- [ ] Switch to userspace and init process

## FAQ

- **Q:** _Why?_

  **A:** Because.

## License

This project is licensed under either Apache License (Version 2.0) or MIT License.
See [LICENSE-APACHE](LICENSE-APACHE) or [LICENSE-MIT](LICENSE-MIT) for details on both.
