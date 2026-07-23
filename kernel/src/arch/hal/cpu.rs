//! Hardware abstraction layer for CPU-specific functionality.

#[inline]
pub fn idle() {
    imp::idle()
}

#[inline]
pub fn halt() -> ! {
    imp::halt()
}

#[inline]
pub fn local_irq_enable() {
    imp::local_irq_enable()
}

#[inline]
pub fn local_irq_disable() {
    imp::local_irq_disable()
}

/// Opaque saved local-IRQ state for this core.
///
/// Produced only by [`local_irq_save`] and consumed only by [`local_irq_restore`].
/// Treat as a token: do not invent values, and restore in LIFO order when nesting.
#[derive(Clone, Copy, Debug)]
pub struct IrqFlags {
    // Intentionally private. Arch backend fills this.
    bits: u64,
}

impl IrqFlags {
    /// Creates an `IrqFlags` from raw bits.
    pub(crate) fn from_raw(bits: u64) -> Self {
        IrqFlags { bits }
    }

    /// Returns the raw bits representing the IRQ state.
    pub(crate) fn to_raw(self) -> u64 {
        self.bits
    }
}

/// Disables local IRQs on this core and returns the previous enable state.
#[inline]
pub fn local_irq_save() -> IrqFlags {
    imp::local_irq_save()
}
/// Restores local IRQ enable state previously returned by [`local_irq_save`].
///
/// # Safety / contract
///
/// - `flags` must come from a matching `local_irq_save` on **this** core.
/// - Nested save/restore must be strictly LIFO.
/// - Between a save and its restore, do not call `local_irq_enable` /
///   `local_irq_disable` (use nested `local_irq_save` instead).
#[inline]
pub fn local_irq_restore(flags: IrqFlags) {
    imp::local_irq_restore(flags)
}

/// Guard that disables interrupts on creation and restores the previous interrupt state on drop.
pub struct LocalIrqGuard {
    flags: IrqFlags,
}

impl Default for LocalIrqGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalIrqGuard {
    /// Disables interrupts and returns a guard that will restore the previous interrupt state on drop.
    #[inline]
    pub fn new() -> Self {
        LocalIrqGuard {
            flags: local_irq_save(),
        }
    }
}

impl Drop for LocalIrqGuard {
    #[inline]
    fn drop(&mut self) {
        local_irq_restore(self.flags);
    }
}

#[inline]
pub fn get_cycles() -> u64 {
    imp::get_cycles()
}

#[inline]
pub fn cycles_per_sec() -> u64 {
    imp::cycles_per_sec()
}

mod imp {
    #[cfg(target_arch = "riscv64")]
    pub use riscv::*;

    #[cfg(target_arch = "riscv64")]
    mod riscv {
        use crate::arch::hal::cpu::IrqFlags;

        #[inline]
        pub fn idle() {
            crate::arch::riscv::idle()
        }

        #[inline]
        pub fn halt() -> ! {
            crate::arch::riscv::halt()
        }

        #[inline]
        pub fn local_irq_enable() {
            crate::arch::riscv::irq::local_irq_enable()
        }

        #[inline]
        pub fn local_irq_disable() {
            crate::arch::riscv::irq::local_irq_disable()
        }

        #[inline]
        pub fn local_irq_save() -> IrqFlags {
            crate::arch::riscv::irq::local_irq_save()
        }

        #[inline]
        pub fn local_irq_restore(flags: IrqFlags) {
            crate::arch::riscv::irq::local_irq_restore(flags)
        }

        #[inline]
        pub fn get_cycles() -> u64 {
            crate::arch::riscv::time::get_cycles()
        }

        #[inline]
        pub fn cycles_per_sec() -> u64 {
            crate::arch::riscv::time::CLINT_TIMEBASE
        }
    }
}
