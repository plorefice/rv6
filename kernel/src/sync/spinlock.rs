use core::ops::{Deref, DerefMut};

use crate::{
    arch::hal::cpu::LocalIrqGuard,
    sync::{Lock, LockPolicy},
};

/// A spinlock that disables interrupts while held.
///
/// This lock is useful for protecting data structures that may be accessed from interrupt context,
/// ensuring that interrupts are disabled while the lock is held.
pub struct IrqSpinLock<T: ?Sized> {
    inner: spin::Mutex<T>,
}

/// A guard that holds an `IrqSpinLock` and restores the previous interrupt state when dropped.
pub struct IrqSpinLockGuard<'a, T: 'a + ?Sized> {
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
    /// Creates a new `IrqSpinLock` protecting the given data.
    pub const fn new(data: T) -> Self {
        IrqSpinLock {
            inner: spin::Mutex::new(data),
        }
    }
}

impl<T: ?Sized> IrqSpinLock<T> {
    /// Locks the `IrqSpinLock`, disabling interrupts and returning a guard that allows access
    /// to the inner data.
    ///
    /// Interrupts will be restored to their previous state when the guard is dropped.
    pub fn lock(&self) -> IrqSpinLockGuard<'_, T> {
        let _irq_guard = LocalIrqGuard::new();
        let guard = self.inner.lock();
        IrqSpinLockGuard { guard, _irq_guard }
    }
}

impl<'a, T: ?Sized> Deref for IrqSpinLockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a, T: ?Sized> DerefMut for IrqSpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<T: ?Sized> Lock for IrqSpinLock<T> {
    type Target = T;

    type Guard<'a>
        = IrqSpinLockGuard<'a, T>
    where
        Self: 'a;

    fn new(data: Self::Target) -> Self
    where
        Self::Target: Sized,
    {
        IrqSpinLock::new(data)
    }

    fn lock(&self) -> Self::Guard<'_> {
        self.lock()
    }
}

/// A lock policy for interrupt context that uses an `IrqSpinLock` for synchronization.
///
/// This lock policy is suitable for protecting data that may be accessed from interrupt context,
/// as it disables interrupts while the lock is held.
pub struct IrqSafe;

impl LockPolicy for IrqSafe {
    type Lock<T: ?Sized> = IrqSpinLock<T>;
}
