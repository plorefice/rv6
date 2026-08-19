# rv6 framebuffer console (fbcon)

This document describes the planned architecture for a framebuffer console in rv6, in the
spirit of Linux's `fbcon`: kernel and userspace output rendered as text onto a graphical
framebuffer, with a keyboard as the input source, replacing the serial port as the primary
interactive console.

It is a design document, not a description of what is implemented. The starting point is
the `ramfb` branch, which has a working framebuffer driver and a virtio input driver that
receives keypresses but does not act on them.

Unless noted otherwise, numbers assume the **QEMU `virt`** machine as configured by the
`justfile` (`-device ramfb`, `-device virtio-keyboard-device`) and the 1024x768 XRGB8888
mode hardcoded by the ramfb driver.

---

## Starting point

Three of the four pieces already exist, but nothing connects them.

| Piece | Location | State |
|-------|----------|-------|
| Framebuffer subsystem | `kernel/src/fb.rs` | `DrawTarget` trait, `Rect`/`Point`/`Pixel`, 8x16 VGA font, global `Once<IrqSpinLock<Framebuffer<Box<dyn DrawTarget>>>>` |
| ramfb driver | `kernel/src/drivers/qemu/ramfb.rs` | 1024x768 XRGB8888 in a `DmaSlice<'static, u32>`; probed from fw_cfg, registers with `fb::register` |
| Line discipline | `kernel/src/tty.rs` | Canonical mode, echo, erase/kill/EOF, blocking reads, abstract `TtyDevice` sink |
| virtio input | `kernel/src/drivers/virtio/input.rs` | IRQ-driven eventq; decodes key **presses** only and `kprintln!`s the keycode |
| Console registry | `kernel/src/console.rs` | Single `Once<Arc<dyn FileOps>>`, claimed by `ns16550` |

So the work is not "write a framebuffer console" so much as inserting two new layers (glyph
rendering and input translation) and untangling who owns the TTY.

Two properties of the current tree shape the design:

- `RiscvDmaAllocator::sync_for_device` is a **no-op** (`kernel/src/arch/riscv/mm/dma.rs`),
  because QEMU `virt` is DMA-coherent and ramfb is polled by QEMU rather than pushed. The
  whole-buffer flush in `RamFb::flush` therefore costs nothing, so damage-rect tracking is
  not a performance requirement — it only starts to matter for a device like virtio-gpu that
  needs real dirty-region uploads.
- `IrqSpinLock` masks local interrupts and increments `ATOMIC_DEPTH`, so `can_sleep()` is
  false while the framebuffer lock is held. Nothing on the render path may block, and it
  should avoid allocating.

---

## Target architecture

```text
 userspace write(1) / read(0)                 keyboard hardware
          │                                          │
   sys_write / sys_read                     virtio-input eventq IRQ
          │                                          │
   FdTable → OpenFile                        input core (dispatch)
          │                                          │
          │                                  keymap + modifier state
          │                                          │
          └──────────────► Tty ◄─────────────────────┘  (+ UART RX IRQ)
                     (line discipline, single instance)
                             │  TtyDevice::write / put / flush
                             ▼
                          TtyMux
                       ┌─────┴─────┐
                       ▼           ▼
                  Ns16550      FbConsole  ← grid, cursor, scroll, attrs
                                    │  DrawTarget: fill_rect / blit / copy_rect / flush
                                    ▼
                          fb::get() → Framebuffer<Box<dyn DrawTarget>>
                                    │
                                 RamFb
```

---

## Design decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Console model | One global `Tty`, many output sinks, many input sources | FDT probe order is incidental, and `Once` silently drops the loser; a mux also keeps the serial console usable during development |
| Rendering context | Draw inline wherever bytes arrive, including IRQ context | No ring buffer or painter thread needed; the seam is preserved so deferral stays a local change |
| Cell attributes | VGA-style attribute byte over a 16-colour palette | ~12 KiB for a 128x48 grid instead of ~73 KiB, and SGR 30–47 maps straight onto palette indices later |

### Console model

The rejected alternative was one `Tty` per device plus priority-based console selection
(Linux's `console=`). It is more faithful to Linux but strictly worse here. `drivers::init`
walks the FDT breadth-first, so whether `ns16550` or fw_cfg/ramfb reaches the console
registry first is incidental — and with a `Once`, the loser is dropped silently. Selection
also means that once fbcon wins, typing at the serial port goes nowhere, which is painful
while bringing the keyboard up.

Inverting ownership so that the console layer creates the `Tty` and drivers merely attach to
it dissolves the ordering problem entirely, because the `Tty` exists before any driver
probes.

### Rendering context

Scrolling 1024x768 by one 16-pixel row is a ~3 MB `memmove` executed with local interrupts
masked, since the framebuffer lives behind an `IrqSpinLock`. That is on the order of
milliseconds of added interrupt latency per scroll, and it is exactly why Linux's fbcon
defers work to a workqueue.

It is tolerable for now, but `FbConsole` should be structured so that its `TtyDevice` impl
does nothing except hand bytes to an internal `ConState` method. Keeping all grid mutation
and drawing behind `&mut ConState` means a future painter kthread — woken through a
`WaitQueue`, fed by a byte ring — can call the same methods from process context without
touching the byte-accepting side. If dropped UART input ever shows up, that becomes a local
change.

---

## Layer 1: `fb` prerequisites

Three gaps in `kernel/src/fb.rs` block fbcon. None change behaviour on their own.

**`Framebuffer<T>` does not expose the drawing primitives.** It offers `clear`,
`draw_pixel`, `draw_text`, `rect` and `flush`, but fbcon needs the raw operations —
particularly `copy_rect` for scrolling. Since `fb::get()` hands out a `Framebuffer` and not
the target, forward them:

```rust
impl<T: DrawTarget> Framebuffer<T> {
    pub fn info(&self) -> FbInfo;
    pub fn fill_rect(&mut self, rect: Rect, color: u32);
    pub fn blit(&mut self, rect: Rect, src: &[u32]);
    pub fn copy_rect(&mut self, src: Rect, dst: Point);
    pub fn flush_rect(&mut self, damage: Rect);
}
```

Forward explicitly rather than exposing a `target_mut()`, so `Framebuffer` stays the policy
layer and `DrawTarget` stays the driver contract.

**The font renders foreground only.** `FramebufferFont::draw_glyph` builds an
`[0u32; 8 * 16]` buffer, sets the pixels where the glyph bitmap has bits, and blits the whole
cell — so unset pixels come out as `0x000000`. That accidentally works for overwriting a cell
on a black background, but it cannot do a coloured background or an inverse-video cursor. Add
an `fg`/`bg` variant and define the existing entry point as `bg = 0`.

**Cell metrics are unreachable.** `CHAR_WIDTH` is a private associated constant and
`char_height` is a private field with no getter. fbcon needs both to compute the grid, so
both need accessors. This is easy to underestimate: it has to land before a grid can be
computed or a cursor drawn.

**`TtyDevice::put` is byte-at-a-time**, which would mean one framebuffer lock acquisition per
character. Add a batch method with a default implementation, leaving `Ns16550` unaffected:

```rust
pub trait TtyDevice: Send + Sync {
    fn put(&self, c: u8);
    fn write(&self, buf: &[u8]) {
        for &b in buf {
            self.put(b);
        }
    }
    fn flush(&self) {}
}
```

`Tty::write` then expands `ONLCR` into a small stack buffer (64 bytes, flushed when full — no
allocation) and calls `device.write` once per chunk. Echo on the receive path stays on `put`,
which is correct: it delivers one or three bytes at a time from IRQ context.

---

## Layer 2: `fbcon`

New module `kernel/src/fbcon.rs`: a character-grid state machine that talks to `fb::get()`
and implements `TtyDevice`.

```rust
pub struct FbConsole {
    state: IrqSpinLock<ConState>,
}

struct ConState {
    cols: usize,              // fb width / font width   → 128 at 1024x768
    rows: usize,              // fb height / font height → 48
    cursor: (usize, usize),   // (col, row)
    cursor_drawn: bool,
    attr: Attr,               // current fg/bg
    damage: Option<Rect>,     // union of cells touched since the last flush
    cells: Vec<Cell>,         // rows * cols shadow buffer
    font: &'static FramebufferFont<'static>,
}

#[derive(Clone, Copy)]
struct Cell {
    ch: u8,
    attr: Attr,
}

/// VGA-style attribute: low nibble foreground, high nibble background,
/// both indices into a static `[u32; 16]` XRGB8888 palette.
#[derive(Clone, Copy)]
struct Attr(u8);
```

The shadow buffer is worth having even though naive scrolling does not need it. It provides
the cell contents under the cursor (so the cursor can be un-inverted without guessing), a
redraw path for targets that cannot `copy_rect`, and the foundation for scrollback and for
multiple virtual consoles.

Operations:

| Operation | Behaviour |
|-----------|-----------|
| `put_byte(b)` | Control-character layer: `\n` (line feed with scroll), `\r`, `\b` (cursor left, no erase), `\t` (next 8-column tab stop), `\x07` (drop), other C0 controls dropped, `0x20..=0xff` printable |
| `write_char(ch)` | Write the cell into the shadow buffer, `draw_glyph` it, advance the cursor, wrap at `cols`, scroll at `rows` |
| `scroll_up(n)` | `copy_rect` the region below row `n` up to `(0, 0)`, `fill_rect` the exposed rows with the background, mirror on the shadow buffer with `cells.copy_within` |
| `show_cursor` / `hide_cursor` | Redraw the cell at the cursor with the attribute nibbles swapped; hide before any mutation, show in `flush` |
| `flush()` | Show the cursor, `fb.flush_rect(damage)`, clear the damage rect |

`TtyDevice for FbConsole` is then thin: `put` takes the lock and does one byte, `write` takes
the lock once and loops, `flush` shows the cursor and flushes damage.

Note that the TTY already emits `BS SP BS` for `ECHOE`, so plain `\b` semantics plus a
printable space is exactly what erase needs. No ANSI parsing is required for `cash` as it
exists today.

---

## Layer 3: input core and keymap

Today the keycode table, the event struct and the event handling all live inside the virtio
driver, and only presses are examined — `handle_event` returns early unless `value == 1`.
That is what makes Shift impossible: the release is never seen. The `KeyCode` enum is Linux
`input-event-codes.h`, not a virtio artifact, so it belongs in a generic layer.

New module `kernel/src/input/`, in three files.

### `input/mod.rs` — event vocabulary and dispatch

```rust
#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    pub kind: EventKind,   // Syn, Key, Rel, Abs, Msc, Led, Rep
    pub code: u16,         // KeyCode when kind == Key
    pub value: i32,        // keys: 0 = release, 1 = press, 2 = repeat
}

pub trait InputDevice: Send + Sync {
    fn name(&self) -> &str;
}

/// Called from IRQ context; implementations must not sleep or allocate.
pub trait InputHandler: Send + Sync {
    fn on_event(&self, ev: InputEvent);
}

pub fn register_device(dev: Arc<dyn InputDevice>) -> InputDevId;
pub fn register_handler(h: Arc<dyn InputHandler>);
pub fn dispatch(ev: InputEvent);
```

A `Vec<Arc<dyn InputHandler>>` under an `IrqSpinLock` is enough, and leaves room for a
magic-sysrq handler later. `register_device` also fixes a latent wart: the `VirtioInputDev`
`Arc` is currently kept alive only because `irq::request_irq` cloned it into the handler
table — `drivers/virtio/mmio.rs` binds it to `_input_dev` and immediately drops it.

### `input/keymap.rs` — keycode to bytes

```rust
bitflags! {
    pub struct Mods: u8 { const SHIFT; const CTRL; const ALT; const CAPS; }
}

pub struct KeyboardState {
    mods: Mods,
}

impl KeyboardState {
    /// Translates one event into 0..N bytes for the line discipline.
    pub fn translate(&mut self, ev: InputEvent, out: &mut KeyBuf);
}

/// Fixed-capacity byte buffer: escape sequences need up to ~6 bytes, and this
/// runs in IRQ context where allocation is unwelcome.
pub struct KeyBuf {
    buf: [u8; 8],
    len: usize,
}
```

Two `[u8; N]` tables indexed by keycode (unshifted and shifted) covering keycodes 1..=83
cover the whole main block. The mappings that matter for the existing termios defaults:

| Key | Keycode | Byte | Why |
|-----|---------|------|-----|
| Enter | 28 | `\r` | `ICRNL` turns it into `\n`, exactly as the serial path does |
| Backspace | 14 | `0x7f` | Matches `Termios::verase` |
| Tab | 15 | `\t` | — |
| Esc | 1 | `0x1b` | Prefix for future escape sequences |

For Ctrl, mask the resulting byte with `& 0x1f` when it lands in `a-z` or `@[\]^_`. That
yields `^D` for EOF and `^U` for kill through the code already in `TtyInput::receive`.

Arrow keys and Home/End want `ESC [ A` style sequences, which is why `KeyBuf` is not a single
byte — but nothing consumes them until there is a VT parser and a shell with history.

### `input/kbd.rs` — bridge to the TTY

```rust
struct KbdHandler {
    state: IrqSpinLock<KeyboardState>,
}

impl InputHandler for KbdHandler {
    fn on_event(&self, ev: InputEvent) {
        let mut buf = KeyBuf::new();
        self.state.lock().translate(ev, &mut buf);
        let tty = console::tty();
        for &b in buf.as_slice() {
            tty.receive_byte(b);
        }
    }
}
```

The virtio driver then shrinks to: read the used buffer, convert `VirtioInputEvent` into
`InputEvent`, call `input::dispatch`. It stops knowing anything about keycodes or consoles,
which is the point.

---

## Layer 4: console ownership

This is the refactor that makes the rest fit together. Today `ns16550` constructs the `Tty`,
owns the RX-to-TTY wiring, and claims the global console. Invert that: the console layer owns
the `Tty`, and drivers attach to it.

```rust
// kernel/src/console.rs
pub struct TtyMux {
    sinks: IrqSpinLock<Vec<Arc<dyn TtyDevice>>>,
}

impl TtyDevice for TtyMux {
    fn put(&self, c: u8)        { for s in self.sinks.lock().iter() { s.put(c) } }
    fn write(&self, buf: &[u8]) { for s in self.sinks.lock().iter() { s.write(buf) } }
    fn flush(&self)             { for s in self.sinks.lock().iter() { s.flush() } }
}

/// Creates the global TTY over an initially empty output mux.
pub fn init();
/// Attaches an output device. Safe to call from driver probe.
pub fn add_output(dev: Arc<dyn TtyDevice>);
/// The system TTY, for drivers pushing received bytes.
pub fn tty() -> Arc<Tty>;
/// Unchanged: what `FdTable::with_stdio` uses.
pub fn get() -> Arc<dyn FileOps>;
```

`Arc<Tty>` coerces to `Arc<dyn FileOps>`, so `OpenFile::console()` in `kernel/src/vfs/fd.rs`
needs no change at all.

`ns16550` keeps its RX IRQ handler but feeds `console::tty().receive_byte(b)` instead of a
TTY it owns, and calls `console::add_output(uart)` in place of `console::register(tty)`.

Serial and framebuffer then stay live simultaneously, and both the UART RX IRQ and the
keyboard handler feed the same line discipline. Among other things, that means `just debug`
over serial keeps working while the keyboard is being brought up.

---

## Layer 5: boot log and panics on screen

`kprintln!` goes unconditionally to the SBI early console (`macros::_print` →
`earlycon::get()`), entirely separately from everything above. Getting the Linux-like effect
of the boot log appearing on the framebuffer needs three pieces:

1. A **kmsg sink list** in the console layer that `_print` also writes to, keeping earlycon
   as the always-on debug lifeline.
2. A **re-entrancy guard** (a single `AtomicBool`) so that a `kprintln!` issued while the
   framebuffer lock is held — from inside `fb`, `fbcon`, or a panic on the render path —
   skips the framebuffer instead of deadlocking on a non-recursive `IrqSpinLock`.
3. A **static early ring buffer** (say 8 KiB) capturing output from the first `kprintln!`
   onward, replayed onto fbcon when it registers. Without this the screen starts mid-boot and
   the logo and driver probe lines are lost.

The panic handler should additionally force a framebuffer flush so the panic message is
visible before the machine stops. That argues for adding `IrqSpinLock::try_lock`, so the
panic path can bypass a held lock rather than hang.

---

## Boot sequence

Because the console layer creates the `Tty` before any driver probes, ordering hazards
disappear. `kmain` becomes:

```rust
console::init();                 // global Tty over an empty mux
irqchip::init(&ctx, &fdt)?;
drivers::init(&ctx, &fdt)?;      // ns16550: add_output + RX → console::tty()
                                 // virtio-input: input::register_device
fbcon::init();                   // if fb::get().is_some() → console::add_output(fbcon)
input::init();                   // register the keyboard handler
sched::init(...);
proc::spawn_kthread(init::kernel_init, fdt_data as usize);
hal::proc::enter_scheduler();
```

`fbcon::init` must run after `drivers::init`, since the framebuffer is registered during
ramfb probe. It is safe for it to allocate the shadow buffer there: the heap is up, and this
is process context.

---

## Concurrency and locking

| Path | Context | Locks taken |
|------|---------|-------------|
| `sys_write` → `Tty::write` → `FbConsole::write` | Process | fb `IrqSpinLock` (once per chunk) |
| Keyboard IRQ → `Tty::receive_byte` → echo | IRQ | TTY input `WaitQueue` (`IrqSafe`), then fb `IrqSpinLock` |
| UART RX IRQ → `Tty::receive_byte` → echo | IRQ | same as above |
| `kprintln!` → kmsg sink → fbcon | Any | re-entrancy guard, then fb `IrqSpinLock` |

The receive path takes the TTY input lock and then the framebuffer lock; the write path takes
only the framebuffer lock. There is no cycle, so no deadlock — only the latency noted above,
since a userspace write masks interrupts for the duration of its chunk.

Constraints that follow from `IrqSpinLock` bumping `ATOMIC_DEPTH`:

- Nothing under the framebuffer lock may block; `can_sleep()` is false there.
- The shadow buffer must be allocated once, at `FbConsole` construction.
- `TtyDevice::put` and `InputHandler::on_event` run in IRQ context and must not allocate.

---

## Milestones

| Milestone | Scope | Done when |
|-----------|-------|-----------|
| **M0** — plumbing | `Framebuffer` forwarding methods; font `fg`/`bg` glyph and public cell metrics; `TtyDevice::write` and chunked `Tty::write` | `just run` behaves exactly as before |
| **M1** — fbcon renders | `kernel/src/fbcon.rs` grid, control chars, scroll, cursor; driven by a temporary call after `drivers::init` | Text appears in the ramfb window, wraps, scrolls, and `\b` + space erases |
| **M2** — console output | `console.rs` refactor; `ns16550` becomes an output sink; fbcon attached | The init banner and `#` prompt appear on screen *and* on serial; serial typing echoes to both |
| **M3** — keyboard input | `kernel/src/input/` core, keycodes, keymap, kbd handler; virtio input driver rewritten | Typing in the QEMU window drives `cash`; Shift and Ctrl work; `^D` exits; Backspace erases on screen |
| **M4** — boot log | kmsg sinks, early ring replay, panic flush | The whole boot log is on screen; a deliberate panic is readable there |

M3 is the point at which init is genuinely running in the fbcon.

---

## Out of scope

Deliberately deferred, roughly in the order they will start to hurt:

- **ANSI/VT subset** in fbcon (colours, cursor addressing, erase-in-line). Not needed for
  `cash` today because the line discipline handles erase with `BS SP BS`, but required before
  any program does more than line-at-a-time output. Parse it in fbcon, as Linux does in
  `vt.c`, rather than in the line discipline.
- **Autorepeat.** QEMU's `virtio-keyboard-device` sends press and release only; Linux
  synthesizes repeats in the input core with a timer. There is no general timer facility in
  the tree beyond the scheduler tick, so hold-to-repeat will not work until one exists.
- **Accurate damage rects.** Free to add, but pointless until a device needs real
  dirty-region uploads (virtio-gpu).
- **Window size to userspace.** A `TIOCGWINSZ`-style ioctl so programs learn they have a
  128x48 grid. There is no ioctl path at all today.
- **Deferred painter kthread.** The mitigation for scroll-induced interrupt latency; the
  `ConState` boundary exists so this stays a local change.
- **`/dev` nodes and multiple virtual consoles** on Alt+Fn, which is also when per-device
  TTYs and real console selection start to make sense.
- **Mode setting.** ramfb is fixed at 1024x768 by the driver, and `FbInfo` has no way to
  query or change modes.

---

## Source of truth

| Topic | Location |
|-------|----------|
| Framebuffer subsystem, font, global registry | `kernel/src/fb.rs` |
| ramfb driver and fw_cfg probe | `kernel/src/drivers/qemu/ramfb.rs`, `fw_cfg.rs` |
| Line discipline, `TtyDevice`, termios | `kernel/src/tty.rs` |
| Console registry | `kernel/src/console.rs` |
| Serial console driver | `kernel/src/drivers/ns16550.rs` |
| virtio input driver | `kernel/src/drivers/virtio/input.rs` |
| virtio-mmio device-id dispatch | `kernel/src/drivers/virtio/mmio.rs` |
| IRQ registration and dispatch | `kernel/src/irq.rs`, `kernel/src/arch/riscv/trap.rs` |
| FDT driver probe | `kernel/src/drivers/mod.rs` |
| `kprintln!` and earlycon | `kernel/src/macros.rs`, `kernel/src/drivers/earlycon.rs` |
| Locks, wait queues, `can_sleep` | `kernel/src/sync.rs`, `kernel/src/sync/spinlock.rs` |
| stdio wiring for new processes | `kernel/src/vfs/fd.rs` |
| DMA coherence assumptions | `kernel/src/arch/riscv/mm/dma.rs` |
| Boot subsystem ordering | `kernel/src/lib.rs` |
| QEMU device flags | `justfile` |
