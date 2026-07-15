# rv6 virtual memory map

This document describes how rv6 splits the RISC-V virtual address space between
kernel and userspace, and what lives in each range.

Unless noted otherwise, numbers assume the **default Sv39** configuration
(`Cargo` feature `sv39`) and the **QEMU `virt`** machine (256 MiB DRAM).

---

## Address-space model (Sv39)

With Sv39, only bits `[38:0]` of a 64-bit VA participate in translation. Bits
`[63:39]` must equal bit 38 (canonical form), which partitions the 64-bit space
into two usable halves with a large non-canonical hole between them:

| Region | Virtual range | Notes |
|--------|---------------|--------|
| Lower half | `0x0000_0000_0000_0000` – `0x0000_003f_ffff_ffff` | 256 GiB; **user** |
| Non-canonical hole | `0x0000_0040_0000_0000` – `0xffff_ffbf_ffff_ffff` | Not usable |
| Upper half | `0xffff_ffc0_0000_0000` – `0xffff_ffff_ffff_ffff` | 256 GiB; **kernel** |

rv6 uses a classic split: the entire lower half is user space and the entire
upper half is kernel space. `USER_TOP` / `KERNEL_BASE` sit at the start of the
upper half. The DRAM direct map lives in a dedicated upper-half window below
the MMIO region.

Canonical constants are defined in `kernel/src/arch/riscv/mm/mod.rs`.

```text
  0xffff_ffff_ffff_ffff  ┌──────────────────────────────┐
                         │  Kernel image (.text … .bss) │
  0xffff_ffff_8000_0000  ├──────────────────────────────┤  LOAD_OFFSET
                         │  MMIO window (bump alloc)    │
  0xffff_ffe0_0000_0000  ├──────────────────────────────┤  IOMAP_MEM_OFFSET
                         │  Direct map of physical DRAM │
  0xffff_ffd0_0000_0000  ├──────────────────────────────┤  PHYS_TO_VIRT_OFFSET
                         │  Kernel heap (grows down)    │
  0xffff_ffc0_0041_2000  ├──────────────────────────────┤  HEAP_MEM_OFFSET
                         │  [guard page]                │
  0xffff_ffc0_0041_1000  ├──────────────────────────────┤  PROC_KSTACK end
                         │  Per-process kernel stack    │
  0xffff_ffc0_0040_1000  ├──────────────────────────────┤  PROC_KSTACK_MEM_OFFSET
                         │  [guard page]                │
  0xffff_ffc0_0040_0000  ├──────────────────────────────┤
                         │  Per-hart kernel stacks      │
  0xffff_ffc0_0000_0000  ├──────────────────────────────┤  KSTACK / USER_TOP /
                         │                              │  KERNEL_BASE
                         │     Sv39 non-canonical hole  │
                         │                              │
  0x0000_0040_0000_0000  ├──────────────────────────────┤  end of lower half
                         │  [guard page]                │
  0x0000_003f_ffff_f000  ├──────────────────────────────┤  user stack top
                         │  User stack (8 MiB)          │
  0x0000_003f_ff7f_f000  ├──────────────────────────────┤  user stack base
                         │  User ELF image & anon maps  │
  0x0000_0000_0000_0000  └──────────────────────────────┘  USER_BASE
```

---

## Split at a glance

| Constant | Address | Role |
|----------|---------|------|
| `USER_BASE` | `0x0000_0000_0000_0000` | Start of user VAS |
| `USER_TOP` / `KERNEL_BASE` | `0xffff_ffc0_0000_0000` | User/kernel policy boundary (start of upper half) |
| `KSTACK_MEM_OFFSET` | `0xffff_ffc0_0000_0000` | Per-hart kernel stacks |
| `PROC_KSTACK_MEM_OFFSET` | `0xffff_ffc0_0040_1000` | Per-process kernel stack |
| `HEAP_MEM_OFFSET` | `0xffff_ffc0_0041_2000` | Low end of kernel heap VA window |
| `PHYS_TO_VIRT_OFFSET` | `0xffff_ffd0_0000_0000` | DRAM direct-map base |
| `IOMAP_MEM_OFFSET` | `0xffff_ffe0_0000_0000` | MMIO mapping window |
| `LOAD_OFFSET` | `0xffff_ffff_8000_0000` | Linked kernel image base |

`KERNEL_BASE` equals `USER_TOP`. Page-table helpers treat any VA `>= KERNEL_BASE`
as “kernel space” when copying kernel mappings into a process address space, and
any VA `< USER_TOP` as “user space” when cloning/destroying user mappings.

---

## Userspace ranges

### `0x0` – `USER_TOP` (`0xffff_ffc0_0000_0000`): user program mappings

**Size:** the full Sv39 lower half (256 GiB of canonical addresses).

**Contents:**

- ELF load segments for user binaries (from initrd / filesystem), mapped with
  `EntryFlags::USER` plus R/W/X derived from the program headers.
- Anonymous mappings created during ELF load (e.g. BSS).
- The per-process user stack (near the high end of the lower half; see below).
- Future `mmap`-style allocations are expected to live here.

PIE binaries currently load at base `0` (`RiscvLoader::choose_pie_base`).

**Page size:** 4 KiB (`PageSize::Kb`).

**Ownership:** private to each process. On `fork`, leaf pages below `USER_TOP`
are deep-copied (`PageTableWalker::clone_user_mappings`), including the user
stack. On process exit they are freed (`destroy_aspace` / `should_free_leaf`).

### User stack: `0x3f_ff7f_f000` – `0x3f_ffff_f000`

**Size:** 8 MiB. Defined in `RiscvProcessMemoryLayout::default_user_stack`
(`kernel/src/arch/riscv/proc.rs`):

- Top (initial SP): `0x0000_003f_ffff_f000`
- Base: top − 8 MiB = `0x0000_003f_ff7f_f000`
- One unmapped page between the stack top and the end of the lower half
  (`0x40_0000_0000`) guards against the non-canonical hole / kernel half.

The stack sits strictly below `USER_TOP`, so `clone_user_mappings` /
`should_free_leaf` treat it as ordinary user space: `fork` copies stack pages
with the rest of the address space and teardown frees them normally.

Flags: `RW | USER | ACCESS` (not `GLOBAL`).

The gap between low ELF/anon mappings and the stack base is unmapped and
available for future growth (`mmap`, heap, …).

---

## Kernel ranges (upper half)

### Per-hart kernel stacks: `KSTACK_MEM_OFFSET`

| | |
|--|--|
| Base | `0xffff_ffc0_0000_0000` |
| Per-hart size | 64 KiB (`KSTACK_MEM_SIZE`) |
| Capacity | 64 harts → 4 MiB total |
| End of region | `0xffff_ffc0_0040_0000` |

Layout helper: `kstack_layout(hart_id)` → `[base + hart_id×64KiB, …)` with
initial SP at the high end.

Today only hart 0 runs; `setup_late` maps a single 64 KiB stack at the base.
Stacks grow downward. A **4 KiB unmapped guard** follows the 64-hart window
before the process kernel stack.

### Per-process kernel stack: `PROC_KSTACK_MEM_OFFSET`

| | |
|--|--|
| Base | `0xffff_ffc0_0040_1000` |
| Size | 64 KiB |
| End | `0xffff_ffc0_0041_1000` |

Used while handling traps/syscalls for the current process. `ThreadInfo`
(kernel SP / user SP) is stored at the **low** end of this stack; the initial
kernel SP is the high end.

Mapped per address space with `RW | ACCESS | GLOBAL` (no `USER`). Unlike other
kernel mappings, these frames are **owned by the process** and freed on
`destroy_aspace`.

Another **4 KiB guard** separates this stack from the heap VA window.

### Kernel heap: `HEAP_MEM_OFFSET` … `PHYS_TO_VIRT_OFFSET`

| | |
|--|--|
| Low bound | `0xffff_ffc0_0041_2000` (`HEAP_MEM_OFFSET`) |
| High bound | `0xffff_ffd0_0000_0000` (`PHYS_TO_VIRT_OFFSET`) |

Implemented as a bump allocator (`HEAP`) that **allocates downward** from
`PHYS_TO_VIRT_OFFSET` toward `HEAP_MEM_OFFSET` (Rust `#[global_allocator]`).

Only the top **1 MiB** is pre-mapped in `setup_late`:

```text
[PHYS_TO_VIRT_OFFSET − 1 MiB, PHYS_TO_VIRT_OFFSET)  →  HEAP_PREALLOC_SIZE
```

i.e. `0xffff_ffcf_fff0_0000` … `0xffff_ffd0_0000_0000`. Further growth would
require on-demand mapping (noted as TODO in code).

### Direct map: `PHYS_TO_VIRT_OFFSET` + DRAM

**Base:** `PHYS_TO_VIRT_OFFSET` = `0xffff_ffd0_0000_0000`.

**Translation:**

```text
va = PHYS_TO_VIRT_OFFSET + (pa − PHYS_MEM_OFFSET)
```

`PHYS_MEM_OFFSET` is the DRAM base from the FDT `memory` node (QEMU `virt`:
typically `0x8000_0000`), stored in `PHYS_MEM_OFFSET` during `setup_late`.

**QEMU `virt` example (−m 256M):**

| Physical | Virtual (direct map) |
|----------|----------------------|
| `0x8000_0000` … `0x9000_0000` | `0xffff_ffd0_0000_0000` … `0xffff_ffd0_1000_0000` |

**Contents:** a linear view of all DRAM so the kernel can:

- Convert frame PPNs to usable pointers (`phys_to_virt`)
- Walk/copy page tables and user pages while the process’s own tables are active
- Access the FDT after early relocation (early setup relocates the FDT pointer
  into this window)

**Mapping:** installed in early boot (`setup_early_vm`) with 2 MiB megapages, then
rebuilt in `setup_late` again with 2 MiB pages and `EntryFlags::KERNEL`
(`RWX | ACCESS | DIRTY | GLOBAL`). Copied into every process page table via
`copy_kernel_mappings` so traps/syscalls can use `phys_to_virt` without
switching back to the global kernel root.

This window is **not** user-accessible (no `USER` PTE bit). The window extends
up to `IOMAP_MEM_OFFSET` (`0xffff_ffe0_0000_0000`), leaving ~64 GiB for DRAM —
far more than QEMU’s default 256 MiB.

### MMIO window: `IOMAP_MEM_OFFSET` … `LOAD_OFFSET`

| | |
|--|--|
| Base | `0xffff_ffe0_0000_0000` |
| End | `0xffff_ffff_8000_0000` (`LOAD_OFFSET`) |

Bump allocator `IOMAP` hands out virtual ranges downward from `LOAD_OFFSET`
toward `IOMAP_MEM_OFFSET`. `RiscvIoMapper::iomap` reserves a VA, then maps the
device’s physical MMIO with `EntryFlags::MMIO` (`RW | ACCESS | DIRTY | GLOBAL`,
4 KiB pages).

Used for PLIC, UART, virtio-mmio, syscon, etc., once discovered from the FDT.

### Kernel image: `LOAD_OFFSET`

| | |
|--|--|
| Link / map base | `0xffff_ffff_8000_0000` |
| Physical load (QEMU + OpenSBI) | typically `0x8020_0000` (DRAM base + 2 MiB) |

Defined identically in `linker/qemu.ld` and `head.S`. The linker places sections
at VAs starting at `LOAD_OFFSET`; `AT(ADDR(...) - LOAD_OFFSET)` records physical
LMAs so the image can be loaded at PA `0x8020_0000` while running at the high VA
after `satp` is enabled.

#### Linker sections (virtual, in order)

| Symbol / section | Contents |
|------------------|----------|
| `_start` / `.head.text` | Linux-style image header + entry (`_start`) |
| `.init.text` | Early init code |
| `_stext` … `_etext` / `.text` | Kernel text |
| `_srodata` … `_erodata` | Read-only data (`.rodata`, `.srodata`) |
| `_sdata` … `_edata` | Early 64 KiB boot stack (`_estack`…`_sstack`) + `.data` / `.sdata` |
| `_sbss` … `_ebss` | `.sbss` / `.bss` (cleared in `head.S`) |
| `_end` | End of kernel image; frame allocator starts after this |

`setup_late` remaps `[_start, _end)` at `LOAD_OFFSET` with 4 KiB `KERNEL` pages.
The early boot stack inside `.data` is only used until the dedicated hart stack
is mapped.

---

## Physical memory (QEMU `virt`)

Not part of the VA map, but it defines what the direct map and frame allocator
cover.

| Physical range | Typical use |
|----------------|-------------|
| `0x0000_0000` – `0x7fff_ffff` | MMIO / devices (PLIC, UART, virtio, …) — mapped on demand into the IOMAP window |
| `0x8000_0000` – … | DRAM (FDT `memory` node; `just run` uses 256 MiB → through `0x8fff_ffff`) |
| `0x8000_0000` | OpenSBI firmware |
| `0x8020_0000` | Kernel image load address (matches image header “load offset” `0x200000`) |
| After `_end` (PA) | Free DRAM for the bump frame allocator (`GFA`) |

Early page-table pages for `setup_early_vm` are carved downward from the **top**
of DRAM before the proper frame allocator exists.

`setup_frame_allocator` sets the bump frame allocator from the physical address
of page-aligned `_end` through the end of the FDT memory region. Subsequent
allocations (page tables, stacks, heap backing, user pages) come from this pool.

---

## Process vs global page tables

| Table | Role |
|-------|------|
| Global kernel root (`MAPPER`) | Built in `setup_late`; used after boot and when no user process is running |
| Per-process root | Allocated in `RiscvLoader::new_user_addr_space` |

Creating a user address space:

1. Allocate an empty root page table.
2. **`copy_kernel_mappings`**: clone every leaf with VA `>= KERNEL_BASE` from the
   global kernel table (direct map, hart stacks, heap, MMIO, kernel image, …).
3. Map the **per-process kernel stack** and **user stack**.
4. Map ELF segments into low user VAs.

Switching into a process writes the process root PPN into `satp`. Kernel code
running on a trap thus still sees the shared high mappings plus that process’s
user pages.

Shared kernel leaves use the `GLOBAL` PTE bit. Process-private kernel stacks
also set `GLOBAL` today but are still tracked as process-owned frames for
teardown.

---

## Boot sequence and mapping lifetime

1. **MMU off** — PC-relative execution at the physical load address; BSS cleared;
   tiny stack in `.data`.
2. **`setup_early_vm`** — static root PTE + early L1 tables; map kernel at
   `LOAD_OFFSET` and DRAM at `PHYS_TO_VIRT_OFFSET` (2 MiB pages); return root PA
   and a direct-mapped FDT pointer.
3. **`relocate` (`head.S`)** — program `satp` for Sv39; continue at high VAs.
4. **`setup_late`** — parse FDT memory; create frame allocator; rebuild a tracked
   root with kernel image, full direct map, hart stack, and 1 MiB heap; install
   it as `MAPPER`.
5. **User processes** — each gets a copied kernel half + private user mappings.

---

## Sv48 note

`sv39` and `sv48` are mutually exclusive Cargo features (default `sv39`). Under
Sv48 the same numeric constants are used; only VPN width / page-table depth and
canonical checks in `VirtAddr` change. The documented ranges remain valid as
long as those constants are unchanged; a true Sv48-wide layout would need
revisiting `USER_TOP` and the upper-half bases.

---

## Source of truth

| Topic | Location |
|-------|----------|
| VA constants & late setup | `kernel/src/arch/riscv/mm/mod.rs` |
| Early mappings | `kernel/src/arch/riscv/mm/init.rs` |
| PTE flags, walkers, clone/destroy | `kernel/src/arch/riscv/mmu.rs` |
| User/kernel stacks, enter-user | `kernel/src/arch/riscv/proc.rs` |
| ELF / addrspace creation | `kernel/src/arch/riscv/mm/elf.rs` |
| MMIO VA allocation | `kernel/src/arch/riscv/mm/mmio.rs` |
| Linker layout | `kernel/src/arch/riscv/linker/qemu.ld` |
| `satp` enable / relocate | `kernel/src/arch/riscv/head.S` |
