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
- Process model / `ProcessBuilder::spawn_user`: `kernel/src/proc/mod.rs`
- Idle + `enqueue_user` / `enter_scheduler`: `kernel/src/arch/riscv/proc.rs`
- Scheduling / park / wake: `kernel/src/proc/sched.rs`
- Sleeping locks: `kernel/src/sync/mutex.rs`, `kernel/src/sync.rs` (`WaitQueue`)

---

## Motivation

`Mutex::lock` and `WaitQueue::wait_until` require `sched::current_process_id()`.
They panic if called from idle or from bare `kmain` with no current process.

Today `kmain` does the following **before** any process exists:

1. Mount ext2 rootfs and read `/init` through VFS (`OpenFile` / `FileOps`).
2. Fall back to initrd if needed.
3. Call `hal::proc::builder().exec(init_code)`, which loads the ELF, enqueues the
   process, then enters `idle_main()` forever.

So VFS and any future `Mutex` around filesystem state cannot be used safely
during boot. Separately, before Phase 0, process creation conflated “spawn
userspace init” with “become the idle loop,” which prevented a kernel thread
from creating a user process and then exiting. Phase 0 splits those.

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

## Current vs target boot flow

```text
Today
─────
  kmain
    ├─ irqchip / drivers / sched::init
    ├─ mount rootfs, read /init     ← no current process
    └─ exec(elf) → enqueue user → idle_main() → switch → return_to_user

Target
──────
  kmain
    ├─ irqchip / drivers / sched::init
    ├─ spawn_kthread(kernel_init)
    └─ enter_scheduler() → idle_main()
                              │
                              ├─ switch → kernel_init
                              │              ├─ mount / load /init
                              │              ├─ spawn_init(elf)
                              │              └─ kthread_exit
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
kernel VAS.”

### Stack and context

- Allocate a per-kthread kernel stack (reuse `PROC_KSTACK` layout / allocator
  used for user process kstacks, or a dedicated kthread stack pool — same VA
  window is fine if indexed by pid/slot).
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
3. Free kthread resources when safe (stack, table slot) — may defer to a reaper
   if freeing while still on that stack is awkward; v1 can park forever in a
   “dead” state or switch to idle then free from idle (implementation detail).
4. `switch` to another runnable task or idle (`None`).

Kernel threads do **not** go through `return_to_user` / `resume_process`’s
`sret`-to-user path. Idle/`switch` already restores `Context` via `swtch`; for
a kthread that previously parked mid-function, resume continues after
`park_*` like a user process that blocked in kernel.

### Spawning

```rust
/// Spawns a kernel thread and enqueues it. Returns its process id.
fn spawn_kthread(entry: fn(usize) -> !, arg: usize) -> ProcessId;

/// Same, but entry may return (trampoline calls kthread_exit).
fn spawn_kthread_fallible(entry: fn(usize), arg: usize) -> ProcessId;
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

Runs as a kernel thread with `current` set:

1. Take the first registered block device from `BLOCK_DEVS`.
2. Mount ext2; `vfs::init_root_fs` on success.
3. Try `root_fs().open("/init")` + `read_to_end`.
4. On failure, load initrd from FDT and find `"init"`.
5. On success, `spawn_init(bytes)` (assigns [`Pid::INIT`], registers init handle).
6. On total failure, panic (or orderly shutdown via syscon).
7. `kthread_exit` (user `/init` continues via idle).

`kmain` after Phase 2:

```text
kmain:
  logo / alloc smoke test
  parse FDT
  irqchip::init
  drivers::init
  sched::init
  spawn_kthread(kernel_init, 0)
  enter_scheduler()   // never returns
```

No VFS in `kmain`.

---

## Lock migration enabled by this work

Once `kernel_init` (and later syscalls) are the only VFS callers:

| Site | Change |
|------|--------|
| `OpenFile::offset` / `FileOps` | `SpinLock<u64>` → `Mutex<u64>` |
| `vfs::ext2::Fs` | `SpinLock<FileSystem<_>>` → `Mutex<_>` (stop spinning across block I/O) |
| MM / IRQ / sched globals | unchanged (`SpinLock` / `IrqSpinLock`) |

Virtio-blk still busy-waits under a queue `SpinLock` today; replacing that with
IRQ + `WaitQueue` is a follow-up and does not block kthreads or the Mutex
migration above.

---

## Implementation checklist

### Phase 0

- [x] Extract `spawn_user` / `enqueue_user` from former `enter_user` / `exec`
- [x] Add `enter_scheduler() -> !` wrapping `idle_main`
- [x] Update `kmain` to `spawn_user` + `enter_scheduler` (dropped `exec`)
- [x] Smoke: `just run` still boots user `/init` (temporarily still from `kmain`
      via `spawn_user` + `enter_scheduler`)

### Phase 2

- [ ] Add `ProcessKind::{User, Kernel}` (or equivalent) to `Process`
- [ ] Kthread stack allocation + `ThreadInfo` setup
- [ ] `kthread_trampoline` + `kthread_exit`
- [ ] `spawn_kthread`
- [ ] Idle/`switch`: confirm SATP/tp for kthreads (kernel tables)
- [ ] Implement `kernel_init`; slim down `kmain`
- [ ] Ensure park/wake/`Mutex` work under `kernel_init` (e.g. contended test later)
- [ ] `just run`: mount, load `/init`, spawn user, kthread exits, user runs

### Follow-ups (not required to merge kthreads)

- [ ] Migrate `FileOps` offset and `ext2::Fs` to `Mutex`
- [ ] Virtio used-buffer IRQ + sleep instead of busy-wait
- [ ] Reap/free kthread stacks and table slots cleanly
- [ ] Hide kernel threads from any user-facing process listing / `wait` targets

---

## Testing plan

1. **Boot regression** — `just initrd && just hddimg && just run` reaches user
   init and console I/O still works.
2. **Initrd fallback** — boot without a usable ext2 `/init` and confirm initrd
   path still runs inside `kernel_init`.
3. **Mutex sanity** — after lock migration, open/read `/init` under `Mutex`
   without panic; optional forced contention later with a second kthread.
4. **Park path** — UART `read` wait queue from user init still wakes (existing
   IRQ `WaitQueue`); kthread may also block on Mutex without wedging idle.

---

## Open implementation details

These can be decided during coding without changing the overall design:

1. **Exact `Process` field layout** — `kind: ProcessKind` vs separate optional
   `kthread: Option<KThreadInfo>` for entry/arg.
2. **Stack VA allocation** — reuse per-process kstack slots vs a small static
   pool for early kthreads.
3. **Exit/reap** — immediate free vs zombie kthread until idle reaps.
4. **HAL surface** — how much of trampoline/`spawn_kthread` lives in
   `arch::riscv::proc` vs generic `proc` (prefer arch for context/stack, generic
   for spawn/exit policy).

---

## Summary

| Phase | Deliverable |
|-------|-------------|
| **0** | `spawn_user` + `enter_scheduler`; `exec` no longer owns the idle loop |
| **2** | `ProcessKind::Kernel`, trampoline, `spawn_kthread`, `kernel_init` owns mount/load/spawn |

Together they make process-context primitives (`Mutex`, park/wake) valid for
boot-time VFS and set up a clean handoff: **kmain → idle → kernel_init → user
/init**.
