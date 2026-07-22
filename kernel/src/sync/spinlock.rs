//! IRQ-safe spinning locks.
//!
//! [`IrqSpinLock`] combines a `spin::Mutex` with [`LocalIrqGuard`](crate::arch::hal::cpu::LocalIrqGuard):
//! local interrupts are disabled before the mutex is taken and restored when the guard is dropped.
//! That prevents the classic deadlock where a process holds the lock, an interrupt runs on the
//! same hart, and the handler tries to take the same lock.
//!
//! [`IrqSafe`] is the [`LockPolicy`](crate::sync::LockPolicy) that selects [`IrqSpinLock`] for
//! generic types such as [`WaitQueue`](crate::sync::WaitQueue).

use core::ops::{Deref, DerefMut};

use crate::{
    arch::hal::cpu::LocalIrqGuard,
    sync::{Lock, LockPolicy},
};

/// Spinlock that masks local interrupts for the duration of the critical section.
///
/// Acquisition order is: disable IRQs, then lock the inner mutex. Release order is the reverse
/// (mutex unlock, then restore IRQs), enforced by [`IrqSpinLockGuard`] field drop order.
///
/// Nesting is supported: each acquisition saves and restores the previous IRQ-enable state via
/// [`LocalIrqGuard`](crate::arch::hal::cpu::LocalIrqGuard).
pub struct IrqSpinLock<T> {
    inner: spin::Mutex<T>,
}

/// Guard for [`IrqSpinLock`]; unlocks the mutex and restores local IRQ state on drop.
///
/// Field order is load-bearing: `guard` must be declared before `_irq_guard` so the mutex is
/// released while interrupts are still disabled.
pub struct IrqSpinLockGuard<'a, T: 'a> {
    // IMPORTANT: keep this field first, so that it is dropped before `_irq_guard`.
    guard: spin::MutexGuard<'a, T>,
    _irq_guard: LocalIrqGuard,
}

impl<T: Default> Default for IrqSpinLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> IrqSpinLock<T> {
    /// Creates an IRQ-safe spinlock protecting `data`.
    pub const fn new(data: T) -> Self {
        IrqSpinLock {
            inner: spin::Mutex::new(data),
        }
    }
}

impl<T> IrqSpinLock<T> {
    /// Disables local IRQs, acquires the lock, and returns a guard for the protected data.
    ///
    /// Interrupts are restored to their previous state when the guard is dropped.
    pub fn lock(&self) -> IrqSpinLockGuard<'_, T> {
        let _irq_guard = LocalIrqGuard::new();
        let guard = self.inner.lock();
        IrqSpinLockGuard { guard, _irq_guard }
    }
}

impl<'a, T> Deref for IrqSpinLockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T> DerefMut for IrqSpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<T> Lock for IrqSpinLock<T> {
    type Target = T;

    type Guard<'a>
        = IrqSpinLockGuard<'a, T>
    where
        Self: 'a;

    fn new(data: Self::Target) -> Self {
        IrqSpinLock::new(data)
    }

    fn lock(&self) -> Self::Guard<'_> {
        self.lock()
    }
}

/// [`LockPolicy`] that uses [`IrqSpinLock`] for every payload type.
///
/// Suitable for shared state accessed from both process context and interrupt handlers.
/// This is the default policy for [`WaitQueue`](super::WaitQueue).
pub struct IrqSafe;

impl LockPolicy for IrqSafe {
    type Lock<T> = IrqSpinLock<T>;
}
