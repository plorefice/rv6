//! Access to various system registers.

use core::arch::asm;

use bitflags::bitflags;

bitflags! {
    /// Flags for the `sstatus` register.
    #[derive(Debug)]
    pub struct SstatusFlags: u64 {
        /// S-Mode interrupt enable.
        const SIE = 1 << 1;
        /// S-Mode previous interrupt enable.
        const SPIE = 1 << 5;
        /// U-Mode big endian memory access.
        const UBE = 1 << 6;
        /// S-Mode previous privilege level.
        const SPP = 1 << 8;
        /// Vector extension unit state.
        const VS = 3 << 9;
        /// Floating point unit state.
        const FS = 3 << 13;
        /// U-Mode extension state.
        const XS = 3 << 15;
        /// Permit S-Mode user memory access.
        const SUM = 1 << 18;
        /// Make executable readable.
        const MXR = 1 << 19;
        /// Value of XLEN for U-Mode.
        const UXL = 3 << 32;
        /// Dirty state presence.
        const SD = 1 << 63;
    }
}

/// The `sstatus` register keeps track of the processor's current operating state.
#[derive(Debug)]
pub struct Sstatus;

impl Sstatus {
    /// Reads the content of `sstatus`.
    #[inline]
    pub fn read() -> SstatusFlags {
        SstatusFlags::from_bits_truncate(Self::read_raw())
    }

    /// Reads the raw content of `sstatus`.
    #[inline]
    pub fn read_raw() -> u64 {
        let value: u64;
        // SAFETY: read with no memory side effects
        unsafe {
            asm!("csrr {}, sstatus", out(reg) value, options(nomem));
        }
        value
    }

    /// Writes flags to `sstatus`.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn write(flags: SstatusFlags) {
        // SAFETY: unsafe op in unsafe fn
        unsafe { Self::write_raw(flags.bits()) }
    }

    /// Writes raw bits to `sstatus`.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn write_raw(flags: u64) {
        // SAFETY: unsafe op in unsafe fn
        unsafe { asm!("csrw sstatus, {}", in(reg) flags, options(nostack)) };
    }

    /// Updates the content of `sstatus`.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn update<F>(f: F)
    where
        F: FnOnce(&mut SstatusFlags),
    {
        let mut v = Self::read();
        f(&mut v);
        // SAFETY: unsafe op in unsafe fn
        unsafe { Self::write(v) };
    }

    /// Sets the specified flags to `sstatus`.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn set(flags: SstatusFlags) {
        // SAFETY: unsafe op in unsafe fn
        unsafe { asm!("csrs sstatus, {}", in(reg) flags.bits(), options(nostack)) };
    }

    /// Clears the specified flags from `sstatus`.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn clear(flags: SstatusFlags) {
        // SAFETY: unsafe op in unsafe fn
        unsafe { asm!("csrc sstatus, {}", in(reg) flags.bits(), options(nostack)) };
    }
}

bitflags! {
    /// Flags for the `sie`/`sip` registers.
    pub struct SiFlags: u64 {
        /// S-Mode software interrupt enable.
        const SSIE = 1 << 1;
        /// S-Mode timer interrupt enable.
        const STIE = 1 << 5;
        /// S-Mode external interrupt enable.
        const SEIE = 1 << 9;
    }
}

/// The `sie` register contains interrupt enable bits.
#[derive(Debug)]
pub struct Sie;

impl Sie {
    /// Reads the content of `sie`.
    #[inline]
    pub fn read() -> SiFlags {
        SiFlags::from_bits_truncate(Self::read_raw())
    }

    /// Reads the raw content of `sie`.
    #[inline]
    pub fn read_raw() -> u64 {
        let value: u64;
        // SAFETY: read with no memory side effects
        unsafe {
            asm!("csrr {}, sie", out(reg) value, options(nomem));
        }
        value
    }

    /// Writes flags to `sie`.
    #[inline]
    pub fn write(flags: SiFlags) {
        Self::write_raw(flags.bits())
    }

    /// Writes raw bits to `sie`.
    #[inline]
    pub fn write_raw(flags: u64) {
        // SAFETY: write with no memory side effects
        unsafe { asm!("csrw sie, {}", in(reg) flags, options(nostack)) };
    }

    /// Updates the content of `sie`.
    #[inline]
    pub fn update<F>(f: F)
    where
        F: FnOnce(&mut SiFlags),
    {
        let mut v = Self::read();
        f(&mut v);
        Self::write(v);
    }

    /// Sets the specified flags to `sie`.
    #[inline]
    pub fn set(flags: SiFlags) {
        // SAFETY: write with no memory side effects
        unsafe { asm!("csrs sie, {}", in(reg) flags.bits(), options(nostack)) };
    }

    /// Clears the specified flags from `sie`.
    #[inline]
    pub fn clear(flags: SiFlags) {
        // SAFETY: write with no memory side effects
        unsafe { asm!("csrc sie, {}", in(reg) flags.bits(), options(nostack)) };
    }
}

/// The `sip` register contains interrupt pending bits.
#[derive(Debug)]
pub struct Sip;

impl Sip {
    /// Reads the content of `sip`.
    #[inline]
    pub fn read() -> SiFlags {
        SiFlags::from_bits_truncate(Self::read_raw())
    }

    /// Reads the raw content of `sip`.
    #[inline]
    pub fn read_raw() -> u64 {
        let value: u64;
        // SAFETY: write with no memory side effects
        unsafe {
            asm!("csrr {}, sip", out(reg) value, options(nomem));
        }
        value
    }

    /// Writes flags to `sip`.
    #[inline]
    pub fn write(flags: SiFlags) {
        Self::write_raw(flags.bits())
    }

    /// Writes raw bits to `sip`.
    #[inline]
    pub fn write_raw(flags: u64) {
        // SAFETY: write with no memory side effects
        unsafe { asm!("csrw sip, {}", in(reg) flags, options(nostack)) };
    }

    /// Updates the content of `sip`.
    #[inline]
    pub fn update<F>(f: F)
    where
        F: FnOnce(&mut SiFlags),
    {
        let mut v = Self::read();
        f(&mut v);
        Self::write(v);
    }

    /// Sets the specified flags to `sip`.
    #[inline]
    pub fn set(flags: SiFlags) {
        // SAFETY: write with no memory side effects
        unsafe { asm!("csrs sip, {}", in(reg) flags.bits(), options(nostack)) };
    }

    /// Clears the specified flags from `sip`.
    #[inline]
    pub fn clear(flags: SiFlags) {
        // SAFETY: write with no memory side effects
        unsafe { asm!("csrc sip, {}", in(reg) flags.bits(), options(nostack)) };
    }
}

/// The `stvec` register holds trap vector configuration.
#[derive(Debug)]
pub struct Stvec;

impl Stvec {
    /// Reads the content of `stvec`.
    #[inline]
    pub fn read() -> u64 {
        let value: u64;
        // SAFETY: read with no memory side effects
        unsafe {
            asm!("csrr {}, stvec", out(reg) value, options(nomem));
        }
        value
    }

    /// Writes to `stvec`.
    #[inline]
    pub fn write(v: u64) {
        // SAFETY: write with no memory side effects
        unsafe { asm!("csrw stvec, {}", in(reg) v, options(nostack)) };
    }
}

/// The `stval` register holds exception-specific information to assist software in handling a trap.
#[derive(Debug)]
pub struct Stval;

impl Stval {
    /// Reads the content of `stval`.
    #[inline]
    pub fn read() -> u64 {
        let value: u64;
        // SAFETY: read with no memory side effects
        unsafe {
            asm!("csrr {}, stval", out(reg) value, options(nomem));
        }
        value
    }

    /// Writes to `stval`.
    #[inline]
    pub fn write(v: u64) {
        // SAFETY: write with no memory side effects
        unsafe { asm!("csrw stval, {}", in(reg) v, options(nostack)) };
    }
}

/// Virtual addressing modes supported by the RISC-V architectures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SatpMode {
    /// `Bare` translation mode (`virt` == `phys`).
    Bare = 0,
    /// `Sv32` translation scheme (2-level page table).
    Sv32 = 1,
    /// `Sv39` translation scheme (3-level page table).
    Sv39 = 8,
    /// `Sv48` translation scheme (4-level page table).
    Sv48 = 9,
}

impl From<u64> for SatpMode {
    fn from(v: u64) -> Self {
        use SatpMode::*;

        match v {
            0 => Bare,
            1 => Sv32,
            8 => Sv39,
            9 => Sv48,
            _ => unreachable!("invalid stval mode field"),
        }
    }
}

/// The `stval` register controls S-Mode address translation and protection.
#[derive(Debug)]
pub struct Satp;

impl Satp {
    /// Reads the physical page number of root page table from the `satp` register.
    #[inline]
    pub fn read_ppn() -> u64 {
        Self::read_raw() & 0xfff_ffff_ffff
    }

    /// Reads the address-space identifier from the `satp` register.
    #[inline]
    pub fn read_asid() -> u64 {
        (Self::read_raw() >> 44) & 0xffff
    }

    /// Reads the virtual translation mode from the `satp` register.
    #[inline]
    pub fn read_mode() -> SatpMode {
        SatpMode::from(Self::read_raw() >> 60)
    }

    /// Reads the raw content of `satp`.
    #[inline]
    pub fn read_raw() -> u64 {
        let value: u64;
        // SAFETY: read with no memory side effects
        unsafe {
            asm!("csrr {}, satp", out(reg) value, options(nomem));
        }
        value
    }

    /// Writes the physical page number of the root page table to the `satp` register.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn write_ppn(ppn: u64) {
        // SAFETY: unsafe op in unsafe fn
        unsafe { Self::write_raw((Self::read_raw() & !0xfff_ffff_ffff_u64) | ppn) }
    }

    /// Writes the address-space identifier to the `satp` register.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn write_asid(asid: u64) {
        let mask = 0xffff << 44;
        // SAFETY: unsafe op in unsafe fn
        unsafe { Self::write_raw((Self::read_raw() & !mask) | (asid << 44)) }
    }

    /// Writes the virtual translation mode to the `satp` register.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn write_mode(mode: SatpMode) {
        let mask = 0xf << 60;
        // SAFETY: unsafe op in unsafe fn
        unsafe { Self::write_raw((Self::read_raw() & !mask) | ((mode as u64) << 60)) }
    }

    /// Writes raw bits to `satp`.
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it's possible to violate memory safety through it.
    #[inline]
    pub unsafe fn write_raw(v: u64) {
        // SAFETY: unsafe op in unsafe fn
        unsafe { asm!("csrw satp, {}", in(reg) v, options(nostack)) }
    }
}

/// The `time` register holds the wall-clock real time that has passed
/// from an arbitrary start time in the past.
#[derive(Debug)]
pub struct Time;

impl Time {
    /// Reads the content of `stval`.
    #[inline]
    pub fn read() -> u64 {
        let value: u64;
        // SAFETY: read with no memory side effects
        unsafe {
            asm!("csrr {}, time", out(reg) value, options(nomem));
        }
        value
    }
}
