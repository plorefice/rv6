//! Implementation of the Supervisor Binary Interface (SBI) specification for RISC-V.
//!
//! This module can be used to interact with an M-mode Runtime Firmware running on a RISC-V machine
//! to execute certain privileged operations in supervisor mode.

use core::{arch::asm, fmt};

/// Prints the SBI version information.
pub fn show_info() {
    let version = base::get_spec_version();

    kprintln!(
        "SBI specification v{}.{} detected",
        version.major,
        version.minor,
    );

    if version.minor != 0x1 {
        kprintln!(
            "SBI implementation ID=0x{:x} Version=0x{:x}",
            base::get_impl_id().unwrap(),
            base::get_impl_version().unwrap(),
        );
        kprint!("SBI v0.2 detected extensions:");
        if base::probe_extension(Extension::Timer).is_ok() {
            kprint!(" TIMER");
        }
        if base::probe_extension(Extension::Ipi).is_ok() {
            kprint!(" IPI");
        }
        if base::probe_extension(Extension::Rfence).is_ok() {
            kprint!(" RFENCE");
        }
        if base::probe_extension(Extension::Hsm).is_ok() {
            kprint!(" HSM");
        }
        if base::probe_extension(Extension::SystemReset).is_ok() {
            kprint!(" SYSRST");
        }
        kprintln!();
    } else {
        panic!("Unsupported SBI specification");
    }
}

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
            _ => unreachable!("unexpected SBI error code"),
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

/// Possible hart states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HartState {
    /// The hart is physically powered-up and executing normally.
    Started,
    /// The hart is not executing in S-Mode or any lower privilege mode.
    Stopped,
    /// Some other hart has requested to start (or power-up) the hart from the `Stopped` state.
    StartPending,
    /// The hart has requested to stop (or power-down) itself from the `Started` state.
    StopPending,
}

impl From<usize> for HartState {
    fn from(code: usize) -> Self {
        match code {
            0 => HartState::Started,
            1 => HartState::Stopped,
            2 => HartState::StartPending,
            3 => HartState::StopPending,
            _ => unreachable!("unexpected hart state code"),
        }
    }
}

/// System reset type being requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResetType {
    /// Physical power down of the entire system.
    Shutdown,
    /// Physical power cycle of the entire system.
    ColdReboot,
    /// Power cycle of main processor and parts of the system.
    WarmReboot,
    /// Implementation-defined reset type.
    Custom(usize),
}

impl From<ResetType> for usize {
    fn from(v: ResetType) -> Self {
        match v {
            ResetType::Shutdown => 0,
            ResetType::ColdReboot => 1,
            ResetType::WarmReboot => 2,
            ResetType::Custom(v) => v,
        }
    }
}

/// Reason for the system reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResetReason {
    /// No reason for reset.
    None,
    /// Unexpected system failure.
    SystemFailure,
    /// Implementation-defined reason.
    Custom(usize),
}

impl From<ResetReason> for usize {
    fn from(v: ResetReason) -> Self {
        match v {
            ResetReason::None => 0,
            ResetReason::SystemFailure => 1,
            ResetReason::Custom(v) => v,
        }
    }
}

macro_rules! ecall {
    ($ext:expr, $fid:expr) => {
        ecall($ext, $fid, 0, 0, 0, 0, 0, 0)
    };
    ($ext:expr, $fid:expr, $a0:expr) => {
        ecall($ext, $fid, $a0, 0, 0, 0, 0, 0)
    };
    ($ext:expr, $fid:expr, $a0:expr, $a1:expr) => {
        ecall($ext, $fid, $a0, $a1, 0, 0, 0, 0)
    };
    ($ext:expr, $fid:expr, $a0:expr, $a1:expr, $a2:expr) => {
        ecall($ext, $fid, $a0, $a1, $a2, 0, 0, 0)
    };
    ($ext:expr, $fid:expr, $a0:expr, $a1:expr, $a2:expr, $a3:expr) => {
        ecall($ext, $fid, $a0, $a1, $a2, $a3, 0, 0)
    };
    ($ext:expr, $fid:expr, $a0:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr) => {
        ecall($ext, $fid, $a0, $a1, $a2, $a3, $a4, 0)
    };
}

/// Namespace for the Base Extension.
pub mod base {
    use super::*;

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

/// Namespace for the Timer Extension.
pub mod timer {
    use super::*;

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

/// Namespace for the IPI Extension.
pub mod ipi {
    use super::*;

    /// Sends an inter-processor interrupt to all the requested harts.
    /// Interprocessor interrupts manifest at the receiving harts as S-Mode software interrupts.
    ///
    /// `hart_mask` is a scalar bit-vector containing hartids.
    /// `hart_mask_base` is the starting hartid from which bit-vector must be computed, or `None`
    /// to indicate that `hart_mask` can be ignored and all available harts must be considered.
    pub fn send_ipi(hart_mask: usize, hart_mask_base: Option<usize>) -> Result<()> {
        let hart_mask_base = hart_mask_base.unwrap_or(usize::MAX);
        ecall!(Extension::Ipi, 0, hart_mask, hart_mask_base).map(|_| ())
    }
}

/// Namespace for the RFENCE extension.
///
/// See [`send_ipi`](crate::ipi::send_ipi) for details on `hart_mask` and `hart_mask_base`.
pub mod rfence {
    use super::*;

    /// Instructs remote harts to execute FENCE.I instruction.
    pub fn remote_fence_i(hart_mask: usize, hart_mask_base: Option<usize>) -> Result<()> {
        let hart_mask_base = hart_mask_base.unwrap_or(usize::MAX);
        ecall!(Extension::Rfence, 0, hart_mask, hart_mask_base).map(|_| ())
    }

    /// Instructs the remote harts to execute one or more SFENCE.VMA instructions,
    /// covering the range of virtual addresses between `start` and `size`.
    pub fn remote_sfence_vma(
        hart_mask: usize,
        hart_mask_base: Option<usize>,
        start: usize,
        size: usize,
    ) -> Result<()> {
        let hart_mask_base = hart_mask_base.unwrap_or(usize::MAX);
        ecall!(Extension::Rfence, 1, hart_mask, hart_mask_base, start, size).map(|_| ())
    }

    /// Instructs the remote harts to execute one or more SFENCE.VMA instructions,
    /// covering the range of virtual addresses between `start` and `size`.
    /// This covers only the given ASID.
    pub fn remote_sfence_vma_asid(
        hart_mask: usize,
        hart_mask_base: Option<usize>,
        start: usize,
        size: usize,
        asid: usize,
    ) -> Result<()> {
        let hart_mask_base = hart_mask_base.unwrap_or(usize::MAX);
        ecall!(
            Extension::Rfence,
            2,
            hart_mask,
            hart_mask_base,
            start,
            size,
            asid
        )
        .map(|_| ())
    }

    /// Instructs the remote harts to execute one or more HFENCE.GVMA instructions, covering
    /// the range of guest physical addresses between `start` and `size` only for the given VMID.
    ///
    /// This function call is only valid for harts implementing hypervisor extension.
    pub fn remote_hfence_gvma_vmid(
        hart_mask: usize,
        hart_mask_base: Option<usize>,
        start: usize,
        size: usize,
        vmid: usize,
    ) -> Result<()> {
        let hart_mask_base = hart_mask_base.unwrap_or(usize::MAX);
        ecall!(
            Extension::Rfence,
            3,
            hart_mask,
            hart_mask_base,
            start,
            size,
            vmid
        )
        .map(|_| ())
    }

    /// Instructs the remote harts to execute one or more HFENCE.GVMA instructions, covering
    /// the range of guest physical addresses between `start` and `size` for all the guests.
    ///
    /// This function call is only valid for harts implementing hypervisor extension.
    pub fn remote_hfence_gvma(
        hart_mask: usize,
        hart_mask_base: Option<usize>,
        start: usize,
        size: usize,
    ) -> Result<()> {
        let hart_mask_base = hart_mask_base.unwrap_or(usize::MAX);
        ecall!(Extension::Rfence, 4, hart_mask, hart_mask_base, start, size).map(|_| ())
    }

    /// Instructs the remote harts to execute one or more HFENCE.VVMA instructions, covering
    /// the range of guest virtual addresses between `start` and `size` for the given ASID and
    /// current VMID (in `hgatp` CSR) of calling hart.
    ///
    /// This function call is only valid for harts implementing hypervisor extension.
    pub fn remote_hfence_vvma_asid(
        hart_mask: usize,
        hart_mask_base: Option<usize>,
        start: usize,
        size: usize,
        asid: usize,
    ) -> Result<()> {
        let hart_mask_base = hart_mask_base.unwrap_or(usize::MAX);
        ecall!(
            Extension::Rfence,
            5,
            hart_mask,
            hart_mask_base,
            start,
            size,
            asid
        )
        .map(|_| ())
    }

    /// Instructs the remote harts to execute one or more HFENCE.VVMA instructions, covering
    /// the range of guest virtual addresses between `start` and `size` for current VMID
    /// (in `hgatp` CSR) of calling hart.
    ///
    /// This function call is only valid for harts implementing hypervisor extension.
    pub fn remote_hfence_vvma(
        hart_mask: usize,
        hart_mask_base: Option<usize>,
        start: usize,
        size: usize,
    ) -> Result<()> {
        let hart_mask_base = hart_mask_base.unwrap_or(usize::MAX);
        ecall!(Extension::Rfence, 6, hart_mask, hart_mask_base, start, size).map(|_| ())
    }
}

/// Namespace for the HSM extension.
pub mod hsm {
    use super::*;

    /// Requests the SBI implementation to start executing the given hart at specified address
    /// in S-Mode.
    ///
    /// This call is asynchronous. More specifically, the function may return before target hart
    /// starts executing, as long as the SBI implemenation is capable of ensuring the return code
    /// is accurate.
    ///
    /// The `start` parameter points to a runtime-specified physical address, where the hart
    /// can start executing in S-Mode. The `opaque` parameter is a XLEN-bit value which
    /// will be set in the `a1` register when the hart starts executing at `start`.
    pub fn start_hart(hartid: usize, start: usize, opaque: usize) -> Result<()> {
        ecall!(Extension::Hsm, 0, hartid, start, opaque).map(|_| ())
    }

    /// Requests the SBI implementation to stop executing the calling hart in S-Mode and return
    /// its ownership to the SBI implementation.
    ///
    /// This call is not expected to return under normal conditions, and must be called
    /// with the S-Mode interrupts disabled.
    pub fn stop_hart() -> Result<()> {
        ecall!(Extension::Hsm, 1).map(|_| ())
    }

    /// Gets the current status (or HSM state) of the given hart.
    pub fn get_hart_status() -> Result<HartState> {
        ecall!(Extension::Hsm, 2).map(HartState::from)
    }
}

/// Namespace for the System Reset extension.
pub mod sysrst {
    use super::*;

    /// Resets the system based on provided type and reason.
    ///
    /// This is a synchronous call and does not return if it succeeds.
    pub fn system_reset(reset_type: ResetType, reset_reason: ResetReason) -> Result<()> {
        ecall!(
            Extension::SystemReset,
            0,
            reset_type.into(),
            reset_reason.into()
        )
        .map(|_| ())
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
