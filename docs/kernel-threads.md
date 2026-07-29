# Kernel threads and boot handoff

Design for introducing **schedulable kernel threads** and splitting user-process
creation from the idle loop, so post-driver boot (rootfs mount, `/init` load)
runs with a real `current` process. That unblocks sleeping locks (`Mutex`) for
VFS and other process-context work.

This document covers **Phase 0** (API split) and **Phase 2** (kernel threads +
boot reorder) from the SpinLock/Mutex migration plan. The temporary “bootstrap
identity” shortcut (Phase 1) is **out of scope** — we go straight to kthreads.

Related code today:

- Boot: `kernel/src/lib.rs` (`kmain`)
- Kernel init thread: `kernel/src/init.rs` (`kernel_init`)
- Process model / `ProcessBuilder::spawn_user`: `kernel/src/proc/mod.rs`
- Idle + `enqueue_user` / `enter_scheduler` / `spawn_kthread`: `kernel/src/arch/riscv/proc.rs`
- Scheduling / park / wake: `kernel/src/proc/sched.rs`
- Sleeping locks: `kernel/src/sync/mutex.rs`, `kernel/src/sync.rs` (`WaitQueue`)

---

## Motivation

`Mutex::lock` and `WaitQueue::wait_until` require `sched::current_process_id()`.
They panic if called from idle or from bare `kmain` with no current process.

Before Phase 2, `kmain` did the following **before** any process existed:

1. Mount ext2 rootfs and read `/init` through VFS (`OpenFile` / `FileOps`).
2. Fall back to initrd if needed.
3. Call into process spawn, which eventually entered `idle_main()` forever.

So VFS and any `Mutex` around filesystem state could not be used safely during
boot. Separately, before Phase 0, process creation conflated “spawn userspace
init” with “become the idle loop,” which prevented a kernel thread from creating
a user process and then exiting. Phase 0 splits those; Phase 2 moves mount/load
into `kernel_init`.

---

## Goals

1. **Phase 0** — Split process creation from scheduler entry:
   - `spawn_user(elf) -> ProcessId` creates and enqueues a user process.
   - `enter_scheduler() -> !` runs `idle_main` and never returns.
2. **Phase 2** — Kernel threads that:
   - Have a `current` process id (so `Mutex` / park / wake work).
   - Run entirely in S-mode on a private kernel stack.
   - Are scheduled like user processes by the existing idle/`switch` path.
3. Reorder boot so `kmain` only finishes driver/sched init, spawns
   `kernel_init`, and enters the idle loop. All mount / `/init` load / user
   spawn happens inside `kernel_init`.
4. Keep **user `/init` as logical `Pid::INIT` (1)** for `wait`/`fork` / orphan
   reparenting. Kernel init must not be mistaken for userspace init.

## Non-goals (first cut)

- SMP / per-hart run queues beyond the existing single-idle model
- Preemption of kernel threads
- Moving FDT driver probe itself under a kthread
- Full POSIX threads or userspace `clone` of kernel threads
- Changing IRQ-safe spinlocks (`PROCESS_TABLE`, PLIC, scheduler, IRQ handlers)
- Virtio completion redesign (separate track; see lock-migration notes below)

---

## Boot flow (implemented)

```text
  kmain
    ├─ irqchip / drivers / sched::init
    ├─ spawn_kthread(kernel_init, fdt_ptr)
    └─ enter_scheduler() → idle_main()
                              │
                              ├─ switch → kernel_init
                              │              ├─ mount / load /init
                              │              ├─ spawn_init(elf)
                              │              └─ return → kthread_exit
                              └─ switch → user /init (return_to_user)
```

---

## Phase 0 — Split spawn from idle

### Problem

Previously, `RiscvUserProcessExecutor::enter_user` both installed the first user
process and called `idle_main()`. Nothing else could enqueue a process and
continue running.

### API (implemented)

| Function | Role |
|----------|------|
| `spawn_user(bytes) -> ProcessId` | On `ProcessBuilder`: build address space, load ELF, set up stacks, call arch `enqueue_user`. Allocates a normal logical `Pid` (≥ 2). Does **not** call idle. |
| `spawn_init(bytes) -> ProcessId` | Like `spawn_user`, but assigns `Pid::INIT` (1) and caches the table handle for orphan reparenting. |
| `enqueue_user(proc, entry, stack) -> ProcessId` | On `UserProcessExecutor`: trap frame / context / kstack `ThreadInfo`, `allocate_process`, `enqueue_process`. |
| `enter_scheduler() -> !` | HAL + riscv: enters `idle_main()` on the boot/idle context. |

Idle remains the only path that calls `switch(None, Some(pid))` for the first
run of a freshly spawned task. `ProcessBuilder::exec` was removed.

### Invariants after Phase 0

- Creating a user process never takes over the CPU permanently.
- `kmain` (and later `kernel_init`) can spawn userspace and return/exit.
- First context switch into a user process still lands in `return_to_user` →
  `resume_process` → `sret`.

Phase 0 alone does not fix Mutex-during-boot; Phase 2 does.

---

## Phase 2 — Kernel threads

### Model

Kernel threads are entries in the existing `PROCESS_TABLE`, distinguished by
kind. They reuse `ProcessState` (`Running` / `Waiting` / `Zombie`), park/wake,
and the round-robin run queue.

```text
ProcessKind::User
  - user address space
  - user + kernel stacks
  - first switch → return_to_user → sret to U-mode
  - syscalls, fork/wait, fds, heap

ProcessKind::Kernel
  - kernel address space only (shared kernel page tables; see below)
  - private kernel stack only (no user stack)
  - first switch → kthread_trampoline → entry(arg) in S-mode
  - may take Mutex, park on WaitQueue, do VFS/block I/O
  - must not return to U-mode; no user `TrapFrame` needed for entry
```

**Recommendation:** add `ProcessKind` (or equivalent) on `Process` rather than a
second table. Park/wake and `current_process_id` stay one code path. Filter
kernel threads out of user-visible “init/parent” semantics later as needed.

### Address space

Two acceptable options (pick one in implementation; document the choice in
code):

1. **Shared kernel page tables** — `aspace` for a kthread is a handle to the
   global kernel root (or a thin wrapper that does not own unique user
   mappings). `switch` loads the same SATP as idle for kthreads.
2. **Dedicated kernel-only AddrSpace** — clone kernel mappings into a private
   root like user processes, but with an empty user half. Heavier; only needed
   if a kthread must isolate kernel mappings (not required for v1).

Prefer (1) for the first cut: less memory, matches “kernel thread = runs in
kernel VAS.” **Implemented:** `RiscvAddrSpace::shared_kernel()` + a
page-aligned private stack allocated from the global heap (`KThreadStack`).

### Stack and context

- Allocate a per-kthread kernel stack from the heap (`PROC_KSTACK_MEM_SIZE`,
  page-aligned via `alloc_zeroed`). Ownership lives on `ProcState::kstack`.
- Place `ThreadInfo` at the stack base as for user processes (`ksp` set, `usp`
  unused / zero).
- Initial `Context`:
  - `sp` = top of kthread stack (16-byte aligned)
  - `ra` = `kthread_trampoline`
  - other callee-saved registers zeroed or holding `arg` per arch convention

### Entry trampoline

```text
kthread_trampoline:
  // current is already set by switch/idle
  call entry(arg)     // or load fn+arg from ThreadInfo / Process
  call kthread_exit   // if entry returns
  // never returns
```

`kthread_exit`:

1. Mark process `Zombie` (or a kernel-specific exited state).
2. `exit_current` / remove from run queue.
3. `switch` to idle (`None`) so reclaim does not run on the dying stack.
4. Idle’s `reap_zombie_kthreads` takes Kernel zombies and `destroy`s them
   (drops the heap `KThreadStack`).

Kernel threads do **not** go through `return_to_user` / `resume_process`’s
`sret`-to-user path. Idle/`switch` already restores `Context` via `swtch`; for
a kthread that previously parked mid-function, resume continues after
`park_*` like a user process that blocked in kernel.

### Spawning

**Implemented:** a single returning entry signature. The trampoline always calls
`kthread_exit` if `entry` returns; there is no separate `fn(usize) -> !` variant.

```rust
/// Spawns a kernel thread and enqueues it. Returns its process id.
/// If `entry` returns, `kthread_trampoline` calls `kthread_exit`.
fn spawn_kthread(entry: fn(usize), arg: usize) -> ProcessId;
```

Exact signatures can use a trait object / closure if the kernel gains a way to
box `'static` callables without undue pain; a function pointer + `usize` arg is
enough for `kernel_init`.

Steps inside `spawn_kthread`:

1. Allocate kernel stack + `ThreadInfo`.
2. Build `Process { kind: Kernel, state: Running, aspace: kernel, ... }`.
3. Set `astate.ctx` for trampoline entry.
4. `allocate_process` + `enqueue_process`.
5. Return pid (do not switch).

### Scheduling interaction

| Path | User process | Kernel thread |
|------|--------------|---------------|
| First run from idle | `switch` → `return_to_user` → `sret` | `switch` → `kthread_trampoline` → `entry` |
| Park / wake | existing | same |
| Yield | existing | same |
| Trap from U-mode | normal | N/A (never in U) |
| Trap from S-mode (IRQ) | existing | existing (same hart S-mode trap path) |

Idle’s `take_next` / `switch(None, Some(pid))` needs **no** special case if the
initial `Context.ra` differs by kind. Optional assert in idle: next process is
`Running`.

### Who is “init”?

- **Kernel init** — first kthread; loads and spawns userspace; then exits.
- **User init** — userspace `/init`, spawned via `spawn_init`; this is the process future
  `wait` without a parent, orphan reparenting, etc. should treat as init.

Logical PIDs (`Pid`) are shared by user and kernel processes and allocated
incrementally (no recycling yet). **`Pid::INIT` (1) is reserved for userspace
init**; other processes (including kthreads) get PIDs starting at 2. Do not assign
userspace meaning to a kthread’s PID. Cache init’s table handle with
`ProcessTable::set_init_id` / `proc::init_process_id()` (done inside `spawn_init`).

---

## `kernel_init` responsibilities

Runs as a kernel thread with `current` set (`kernel/src/init.rs`):

1. Take the first registered block device from `BLOCK_DEVS` (or fall through if
   none).
2. Mount ext2; `vfs::init_root_fs` on success.
3. Try `root_fs().open("/init")` + `read_to_end`.
4. On failure, load initrd from FDT and find `"init"`.
5. On success, `spawn_init(bytes)` (assigns [`Pid::INIT`], registers init handle).
6. On total failure, panic (or orderly shutdown via syscon).
7. Return from entry → `kthread_trampoline` → `kthread_exit` (user `/init`
   continues via idle).

`kmain` after Phase 2:

```text
kmain:
  logo / alloc smoke test
  parse FDT
  irqchip::init
  drivers::init
  sched::init
  spawn_kthread(kernel_init, fdt_ptr)
  enter_scheduler()   // never returns
```

No VFS in `kmain`.

---

## Lock migration

With `kernel_init` (and later syscalls) as the VFS callers, the following are
**done**:

| Site | Change |
|------|--------|
| `OpenFile::offset` / `FileOps` | `SpinLock<u64>` → `Mutex<u64>` |
| `vfs::ext2::Fs` | `SpinLock<FileSystem<_>>` → `Mutex<_>` (stop spinning across block I/O) |
| MM / IRQ / sched globals | unchanged (`SpinLock` / `IrqSpinLock`) |

Virtio-blk still busy-waits under a queue `SpinLock` today; replacing that with
IRQ + `WaitQueue` remains a follow-up.

---

## Implementation checklist

### Phase 0

- [x] Extract `spawn_user` / `enqueue_user` from former `enter_user` / `exec`
- [x] Add `enter_scheduler() -> !` wrapping `idle_main`
- [x] Update `kmain` to `spawn_user` + `enter_scheduler` (dropped `exec`)
- [x] Smoke: `just run` still boots user `/init` (temporarily still from `kmain`
      via `spawn_user` + `enter_scheduler`)

### Phase 2

- [x] Add `ProcessKind::{User, Kernel}` (or equivalent) to `Process`
- [x] Kthread stack allocation + `ThreadInfo` setup
- [x] `kthread_trampoline` + `kthread_exit`
- [x] `spawn_kthread`
- [x] Idle/`switch`: confirm SATP/tp for kthreads (kernel tables)
- [x] Implement `kernel_init`; slim down `kmain`
- [x] Ensure park/wake/`Mutex` work under `kernel_init` (contended hold-across-yield
      smoke test exercised during bring-up)
- [x] `just run`: mount, load `/init`, spawn user, kthread exits, user runs

### Follow-ups (not required to merge kthreads)

- [x] Migrate `FileOps` offset and `ext2::Fs` to `Mutex`
- [ ] Virtio used-buffer IRQ + sleep instead of busy-wait
- [x] Reap/free kthread stacks and table slots cleanly (idle + heap `Drop`)
- [ ] Hide kernel threads from any user-facing process listing / `wait` targets

---

## Testing plan

1. **Boot regression** — `just initrd && just hddimg && just run` reaches user
   init and console I/O still works. **Verified** (rootfs `/init` path).
2. **Initrd fallback** — boot without a usable ext2 `/init` (no virtio-blk) and
   confirm initrd path still runs inside `kernel_init`. **Verified**.
3. **Mutex sanity** — open/read `/init` under `Mutex` without panic; contended
   hold-across-yield between two kthreads parks and wakes correctly. **Verified**
   during bring-up (temporary test removed afterward).
4. **Park path** — UART `read` wait queue from user init still wakes (existing
   IRQ `WaitQueue`); kthread may also block on Mutex without wedging idle.

---

## Open implementation details

These can be decided during coding without changing the overall design:

1. **Exact `Process` field layout** — **done:** `kind: ProcessKind` plus
   `ThreadInfo::{entry, arg}` for kthread entry.
2. **Stack VA allocation** — **done:** heap-backed `KThreadStack` under shared root
   (no fixed slot pool).
3. **Exit/reap** — **done:** `kthread_exit` → idle; idle reaps Kernel zombies.
4. **HAL surface** — **done:** trampoline/`spawn_kthread` live in
   `arch::riscv::proc`; generic `proc` exposes `spawn_kthread` / `kthread_exit` /
   `reap_zombie_kthreads`.

### S-mode trap entry (fixed)

`trap_entry` branches on trap origin (`sscratch == 0` ⇒ S-mode):

- **U-mode** — stash user `sp` in `ThreadInfo::usp`, load `ThreadInfo::ksp`,
  push the trap frame on the process kernel stack (unchanged).
- **S-mode** — keep the interrupted `sp` and nest the trap frame on the current
  stack. `ThreadInfo` is left untouched. Before calling `handle_exception`,
  reload the interrupted frame pointer into `s0` so `unwind_stack_frame` can
  walk into the interrupted frames. This preserves kthread (and in-kernel
  user-process) frames for diagnostics, and is a prerequisite for kthread
  preemption. Interrupts remain masked while kthreads run today; enabling
  `SIE` for kthreads is still a separate follow-up.

Because S-mode traps now consume the interrupted stack, a fault raised *while*
reporting a fault would recurse until the stack is exhausted. Two guards
prevent that: `walk_stack_frame` validates each frame pointer (kernel address,
aligned, strictly ascending, bounded frame count) instead of trusting the
chain, and `handle_exception` halts on the second fatal exception rather than
re-running the diagnostics.

---

## Summary

| Phase | Deliverable |
|-------|-------------|
| **0** | `spawn_user` + `enter_scheduler`; `exec` no longer owns the idle loop |
| **2** | `ProcessKind::Kernel`, trampoline, `spawn_kthread`, `kernel_init` owns mount/load/spawn |

Together they make process-context primitives (`Mutex`, park/wake) valid for
boot-time VFS and set up a clean handoff: **kmain → idle → kernel_init → user
/init**.
