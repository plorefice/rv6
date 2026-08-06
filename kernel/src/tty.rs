//! Terminal (TTY) layer: line discipline over a byte-oriented device.
//!
//! This module provides:
//!
//! - [`TtyDevice`] — raw output sink implemented by a driver (UART, framebuffer, …)
//! - [`Tty`] — line discipline plus [`FileOps`] for userspace read/write
//! - [`Termios`] / [`IFlags`] / [`OFlags`] / [`LFlags`] — processing and editing controls
//!
//! Drivers feed received bytes with [`Tty::receive_byte`] (typically from an IRQ handler).
//! Userspace talks to the same [`Tty`] through the VFS console/`FileOps` path.
//!
//! # Layering
//!
//! | Piece | Role |
//! |-------|------|
//! | [`TtyDevice`] | Hardware (or virtual) output: [`put`](TtyDevice::put) / [`flush`](TtyDevice::flush) |
//! | [`Tty`] | Input processing, echo, line editing, output post-processing, blocking reads |
//!
//! The device must not interpret termios; the TTY owns that policy. Echo and
//! [`FileOps::write`] both call into the device.
//!
//! # Canonical vs non-canonical input
//!
//! Controlled by [`LFlags::ICANON`] in [`Termios::lflag`]:
//!
//! | Mode | Buffering | Editing | When readers wake |
//! |------|-----------|---------|-------------------|
//! | Canonical (`ICANON`) | pending line in an edit buffer | erase / kill / EOF | line commit (`\n` or `VEOF`) or empty-line EOF |
//! | Non-canonical | each byte goes to the ready queue | none | every accepted byte |
//!
//! Canonical [`FileOps::read`] stops at a newline when one is present. Non-canonical reads
//! drain whatever is ready, up to the caller's buffer.
//!
//! # EOF (`VEOF`, typically `^D`)
//!
//! In canonical mode, [`Termios::veof`] does **not** insert a character into the line:
//!
//! | Line state | Effect |
//! |------------|--------|
//! | Empty | Next `read` returns `Ok(0)` (end-of-file); the EOF latch is then cleared |
//! | Non-empty | Commits the pending line **without** a trailing `\n` (no kernel echo of a newline) |
//!
//! Interactive programs that need the cursor on the next row after a partial-line `VEOF`
//! should print a newline themselves when the returned data has no trailing `\n`.
//!
//! # Interrupt context
//!
//! [`Tty::receive_byte`] may run from an IRQ handler. It locks IRQ-safe state
//! ([`WaitQueue`] / [`IrqSpinLock`]) and may call [`TtyDevice::put`] for echo, so device
//! `put` must not sleep or allocate. The ready queue is capped; overflow drops bytes rather
//! than growing under the IRQ path.
//!
//! [`FileOps`] methods run only in process context (they may block on the input wait queue).

use core::io::SeekFrom;

use alloc::{collections::VecDeque, sync::Arc};
use bitflags::bitflags;
use uapi::Errno;

use crate::{
    sync::{IrqSpinLock, Mutex, WaitQueue},
    vfs::file_ops::FileOps,
};

/// Maximum length of the canonical edit buffer (bytes).
const LINE_MAX: usize = 256;

/// Cap on the ready queue delivered to readers; further input is dropped.
const READY_CAP: usize = 512;

/// Byte-oriented output device backing a [`Tty`].
///
/// Implementations may be called from interrupt context (echo), so [`put`](TtyDevice::put)
/// must not sleep or allocate.
pub trait TtyDevice: Send + Sync {
    /// Writes a single byte to the device.
    fn put(&self, c: u8);

    /// Makes previously written bytes visible, if the device batches them.
    ///
    /// Serial drivers need no flush; a framebuffer console uses this to push the
    /// dirty region to the display once per write instead of once per glyph.
    fn flush(&self) {}
}

/// Line discipline and console file object over a [`TtyDevice`].
///
/// Holds termios settings, the canonical edit buffer / ready queue, and implements
/// [`FileOps`] for blocking reads and post-processed writes. Construct with [`Tty::new`],
/// register as the console (or open via the VFS), and feed RX with [`Tty::receive_byte`].
pub struct Tty {
    device: Arc<dyn TtyDevice>,
    input: WaitQueue<TtyInput>,
    termios: IrqSpinLock<Termios>,
}

impl Tty {
    /// Creates a TTY over `device` with interactive defaults.
    ///
    /// Default [`Termios`]: `ICRNL` on input; `OPOST | ONLCR` on output; canonical mode with
    /// `ECHO | ECHOE`; erase = `DEL` (`0x7f`), kill = `^U`, EOF = `^D`.
    pub fn new(device: Arc<dyn TtyDevice>) -> Self {
        Self {
            device,
            input: WaitQueue::new(TtyInput {
                line: LineBuf::new(),
                ready: VecDeque::with_capacity(READY_CAP),
                eof: false,
            }),
            termios: IrqSpinLock::new(Termios {
                iflag: IFlags::ICRNL,
                oflag: OFlags::OPOST | OFlags::ONLCR,
                lflag: LFlags::ICANON | LFlags::ECHO | LFlags::ECHOE,
                verase: 0x7f, // DEL
                vkill: 0x15,  // ^U
                veof: 0x04,   // ^D
            }),
        }
    }

    /// Feeds a received byte into the line discipline.
    ///
    /// Applies input flags, canonical editing / echo as configured by [`Termios`], and wakes
    /// blocked readers when data or EOF becomes available.
    ///
    /// May be called from interrupt context; see the [module overview](crate::tty).
    pub fn receive_byte(&self, byte: u8) {
        let t = *self.termios.lock();
        let mut input = self.input.lock();
        let mut echo = |c| self.device.put(c);
        if matches!(input.receive(byte, &t, &mut echo), Feed::Readable) {
            input.wake_all();
        }
        self.device.flush();
    }
}

impl FileOps for Tty {
    fn read(&self, _off: &Mutex<u64>, buf: &mut [u8]) -> Result<usize, Errno> {
        let canon = self.termios.lock().lflag.contains(LFlags::ICANON);

        let mut input = self.input.wait_until(|i| !i.ready.is_empty() || i.eof);

        if input.ready.is_empty() {
            input.eof = false; // reset EOF for next read
            return Ok(0);
        }

        let mut n = 0;
        while n < buf.len() {
            let Some(b) = input.ready.pop_front() else {
                break;
            };
            buf[n] = b;
            n += 1;
            if canon && b == b'\n' {
                break;
            }
        }
        Ok(n)
    }

    fn write(&self, _off: &Mutex<u64>, buf: &[u8]) -> Result<usize, Errno> {
        let t = *self.termios.lock();
        let nlcr = t.oflag.contains(OFlags::OPOST) && t.oflag.contains(OFlags::ONLCR);

        for &b in buf {
            if nlcr && b == b'\n' {
                self.device.put(b'\r');
            }
            self.device.put(b);
        }
        self.device.flush();
        Ok(buf.len())
    }

    fn seek(&self, _off: &Mutex<u64>, _whence: SeekFrom) -> Result<u64, Errno> {
        Err(Errno::NoSys)
    }
}

/// Input state shared between the receive path (e.g. IRQ context) and readers.
struct TtyInput {
    /// Line currently being edited; only used in canonical mode.
    line: LineBuf,
    /// Bytes ready to be delivered to readers, oldest first.
    ready: VecDeque<u8>,
    /// Empty-line `VEOF` was seen; the next `read` returns `Ok(0)`.
    eof: bool,
}

impl TtyInput {
    fn receive(&mut self, byte: u8, t: &Termios, echo: &mut impl FnMut(u8)) -> Feed {
        let byte = match byte {
            b'\r' if t.iflag.contains(IFlags::IGNCR) => return Feed::Pending,
            b'\r' if t.iflag.contains(IFlags::ICRNL) => b'\n',
            b => b,
        };
        if !t.lflag.contains(LFlags::ICANON) {
            self.push_ready(byte);
            if t.lflag.contains(LFlags::ECHO) {
                echo(byte);
            }
            return Feed::Readable;
        }
        match byte {
            b if b == t.verase => {
                self.erase(1, t, echo);
                Feed::Pending
            }
            b if b == t.vkill => {
                self.erase(self.line.len, t, echo);
                Feed::Pending
            }
            b if b == t.veof => self.commit_eof(),
            b'\n' => {
                if t.lflag.contains(LFlags::ECHO) {
                    echo(b'\r');
                    echo(b'\n');
                }
                self.commit_line(Some(b'\n'))
            }
            b => {
                if self.line.push(b) && t.lflag.contains(LFlags::ECHO) {
                    echo(b);
                }
                Feed::Pending
            }
        }
    }

    fn push_ready(&mut self, byte: u8) {
        if self.ready.len() < READY_CAP {
            self.ready.push_back(byte);
        }
    }

    fn erase(&mut self, n: usize, t: &Termios, echo: &mut impl FnMut(u8)) {
        for _ in 0..n {
            if self.line.pop().is_none() {
                break; // never erase past the start of the line — that's the prompt
            }
            if t.lflag.contains(LFlags::ECHOE) {
                echo(0x08);
                echo(b' ');
                echo(0x08);
            }
        }
    }

    fn commit_line(&mut self, nl: Option<u8>) -> Feed {
        self.ready.extend(self.line.chunk());
        self.line.clear();
        if let Some(nl) = nl {
            self.push_ready(nl);
        }
        Feed::Readable
    }

    fn commit_eof(&mut self) -> Feed {
        if self.line.len() == 0 {
            self.eof = true;
            Feed::Readable
        } else {
            self.commit_line(None)
        }
    }
}

/// Fixed-capacity canonical edit buffer (no allocation on push/pop).
struct LineBuf {
    buf: [u8; LINE_MAX],
    len: usize,
}

impl LineBuf {
    fn new() -> Self {
        Self {
            buf: [0; LINE_MAX],
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) -> bool {
        if self.len >= LINE_MAX {
            return false;
        }
        self.buf[self.len] = byte;
        self.len += 1;
        true
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.buf[self.len])
    }

    fn chunk(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    fn len(&self) -> usize {
        self.len
    }

    fn clear(&mut self) {
        self.buf = [0; LINE_MAX];
        self.len = 0;
    }
}

bitflags! {
    /// Input processing flags ([`Termios::iflag`]).
    #[derive(Clone, Copy)]
    pub struct IFlags: u32 {
        /// Translate CR to NL on input.
        const ICRNL = 1 << 0;
        /// Discard CR on input (checked before [`ICRNL`](IFlags::ICRNL)).
        const IGNCR = 1 << 1;
    }

    /// Output post-processing flags ([`Termios::oflag`]).
    ///
    /// Applied on [`FileOps::write`]. Echo of input newlines always emits CR-LF when
    /// [`LFlags::ECHO`] is set, independent of these flags.
    #[derive(Clone, Copy)]
    pub struct OFlags: u32 {
        /// Enable output post-processing.
        const OPOST = 1 << 0;
        /// Translate NL to CR-NL on output (requires [`OPOST`](OFlags::OPOST)).
        const ONLCR = 1 << 1;
    }

    /// Local mode flags ([`Termios::lflag`]).
    #[derive(Clone, Copy)]
    pub struct LFlags: u32 {
        /// Canonical mode: line buffering and editing.
        const ICANON = 1 << 0;
        /// Echo input characters.
        const ECHO = 1 << 1;
        /// Erase characters visibly (`BS SP BS`) when [`Termios::verase`] is received.
        const ECHOE = 1 << 2;
    }
}

/// Terminal processing attributes for a [`Tty`].
///
/// A subset of POSIX termios: input/output/local flags plus the erase, kill, and EOF
/// special characters. Defaults are chosen in [`Tty::new`]. There is not yet an ioctl path
/// to change these from userspace.
#[derive(Clone, Copy)]
pub struct Termios {
    /// Input flags ([`IFlags`]).
    pub iflag: IFlags,
    /// Output flags ([`OFlags`]).
    pub oflag: OFlags,
    /// Local mode flags ([`LFlags`]).
    pub lflag: LFlags,
    /// Erase character (default `DEL` / `0x7f`); deletes one byte from the edit buffer.
    pub verase: u8,
    /// Kill character (default `^U` / `0x15`); clears the entire edit buffer.
    pub vkill: u8,
    /// EOF character (default `^D` / `0x04`); see the [module EOF section](crate::tty).
    pub veof: u8,
}

/// Outcome of feeding one byte to the line discipline.
enum Feed {
    /// Byte was buffered or consumed by editing; readers stay blocked.
    Pending,
    /// New data is available in `ready`, or EOF is latched; wake readers.
    Readable,
}
