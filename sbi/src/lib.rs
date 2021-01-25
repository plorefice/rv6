//! Implementation of the Supervisor Binary Interface (SBI) specification for RISC-V.
//!
//! This crate can be used to interact with an M-mode Runtime Firmware running on a RISC-V machine
//! to execute certain privileged operations in supervisor mode.

#![no_std]
#![warn(missing_docs)]
#![deny(missing_debug_implementations)]
#![feature(asm)]

use core::fmt;

/// A standard SBI error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SbiError {
    /// Operation failed.
    Failed,
    /// Operation not supported.
    NotSupported,
    /// Invalid parameters in request.
    InvalidParam,
    /// Permission denied.
    Denied,
    /// Invalid address.
    InvalidAddress,
    /// Already available.
    AlreadyAvailable,
}

impl From<isize> for SbiError {
    fn from(code: isize) -> Self {
        match code {
            -1 => SbiError::Failed,
            -2 => SbiError::NotSupported,
            -3 => SbiError::InvalidParam,
            -4 => SbiError::Denied,
            -5 => SbiError::InvalidAddress,
            -6 => SbiError::AlreadyAvailable,
            _ => unreachable!("invalid SBI error code"),
        }
    }
}

impl fmt::Display for SbiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SbiError::Failed => "operation failed",
                SbiError::NotSupported => "operation not supported",
                SbiError::InvalidParam => "invalid parameter",
                SbiError::Denied => "operation not permitted",
                SbiError::InvalidAddress => "invalid address",
                SbiError::AlreadyAvailable => "already available",
            }
        )
    }
}

/// Result type for SBI operations.
pub type Result<T> = core::result::Result<T, SbiError>;

/// SBI extensions as defined by the current SBI specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum Extension {
    /// Legacy Set Timer operation.
    LegacyTimer = 0x00,
    /// Legacy Console Putchar operation.
    LegacyPutChar = 0x01,
    /// Legacy Console Getchar operation.
    LegacyGetChar = 0x02,
    /// Legacy Clear IPI operation.
    LegacyClearIPI = 0x03,
    /// Legacy Send IPI operation.
    LegacySendIPI = 0x04,
    /// Legacy Remote FENCE.I operation.
    LegacyRemoteFence = 0x05,
    /// Legacy Remote SFENCE.VMA operation.
    LegacyRemoteSFence = 0x06,
    /// Legacy Remote SFENCE.VMA with ASID operation.
    LegacyRemoteSFenceASID = 0x07,
    /// Legacy System Shutdown operation.
    LegacySystemShutdown = 0x08,

    /// Base Extension.
    Base = 0x10,
    /// Timer Extension.
    Timer = 0x54494D45,
    /// IPI Extension.
    Ipi = 0x735049,
    /// RFENCE Extension.
    Rfence = 0x52464E43,
    /// Hart State Management Extension.
    Hsm = 0x48534D,
    /// System Reset Extension.
    SystemReset = 0x53525354,
}

/// SBI specification version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpecVersion {
    /// Major number of the SBI spec.
    pub major: usize,
    /// Minor number of the SBI spec.
    pub minor: usize,
}

macro_rules! ecall {
    ($ext:expr, $fid:expr) => {
        ecall($ext, $fid, 0, 0, 0, 0, 0, 0)
    };
    ($ext:expr, $fid:expr, $a0:expr) => {
        ecall($ext, $fid, $a0, 0, 0, 0, 0, 0)
    };
    ($ext:expr, $fid:expr, $a0:expr, $a1: expr) => {
        ecall($ext, $fid, $a0, $a1, 0, 0, 0, 0)
    };
}

/// SBI Base Extension.
#[derive(Debug)]
pub struct Base;

impl Base {
    /// Returns the current SBI specification version.
    pub fn get_spec_version() -> SpecVersion {
        let v = ecall!(Extension::Base, 0).unwrap();
        SpecVersion {
            major: (v >> 24) & 0x7f,
            minor: v & 0xffffff,
        }
    }

    /// Returns the current SBI implementation ID, which is different for every SBI implementation.
    ///
    /// The implementation ID allows software to probe for SBI implementation quirks.
    pub fn get_impl_id() -> Result<usize> {
        ecall!(Extension::Base, 1)
    }

    /// Returns the current SBI implementation version.
    ///
    /// The encoding of this version number is specific to the SBI implementation.
    pub fn get_impl_version() -> Result<usize> {
        ecall!(Extension::Base, 2)
    }

    /// Returns zero if the given SBI extension ID (EID) is not available, or an extension-specific
    /// non-zero value if it is available.
    pub fn probe_extension(id: Extension) -> Result<usize> {
        ecall!(Extension::Base, 3, id as usize)
    }

    /// Returns a value that is legal for the `mvendorid` CSR.
    ///
    /// Zero is always a legal value for this CSR.
    pub fn get_mvendorid() -> Result<usize> {
        ecall!(Extension::Base, 4)
    }

    /// Returns a value that is legal for the `marchid` CSR.
    ///
    /// Zero is always a legal value for this CSR.
    pub fn get_marchid() -> Result<usize> {
        ecall!(Extension::Base, 5)
    }

    /// Returns a value that is legal for the `mimpid` CSR.
    ///
    /// Zero is always a legal value for this CSR.
    pub fn get_mimpid() -> Result<usize> {
        ecall!(Extension::Base, 6)
    }
}

/// SBI Timer Extension.
#[derive(Debug)]
pub struct Timer;

impl Timer {
    /// Programs the clock for next event after `stime` time, in absolute time.
    ///
    /// If the supervisor wishes to clear the timer interrupt without scheduling the next
    /// timer event, it can either request a timer interrupt infinitely far into the future
    /// (ie. `uint64::MAX`), or it can instead mask the timer interrupt by clearing `sie.STIE` bit.
    pub fn set_timer(stime: u64) -> Result<()> {
        if cfg!(target_pointer_width = "32") {
            ecall!(Extension::Timer, 0, stime as usize, (stime >> 32) as usize).map(|_| ())
        } else {
            ecall!(Extension::Timer, 0, stime as usize).map(|_| ())
        }
    }
}

/// Low-level syscall to invoke an operation over SBI.
#[allow(clippy::too_many_arguments)]
fn ecall(
    ext: Extension,
    fid: usize,
    mut a0: usize,
    mut a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> Result<usize> {
    unsafe {
        asm!("ecall",
                inout("a0") a0,
                inout("a1") a1,
                in("a2") a2,
                in("a3") a3,
                in("a4") a4,
                in("a5") a5,
                in("a6") fid,
                in("a7") ext as usize);
    }

    if a0 == 0 {
        Ok(a1)
    } else {
        Err(SbiError::from(a0 as isize))
    }
}
