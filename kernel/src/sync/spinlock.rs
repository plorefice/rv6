//! Spinning locks.
//!
//! This module provides two exclusive locks that **spin** on contention (they never sleep):
//!
//! - [`SpinLock`] — plain spinlock; local interrupts stay enabled
//! - [`IrqSpinLock`] — disables local IRQs for the critical section, then takes a [`SpinLock`]
//!
//! # Which lock to use
//!
//! | Lock | Disables local IRQs? | Safe from IRQ handlers? |
//! |------|----------------------|-------------------------|
//! | [`IrqSpinLock`] / [`IrqSafe`] | yes | yes |
//! | [`SpinLock`] / [`ProcessContext`](crate::sync::ProcessContext) | no | **no** |
//!
//! Use [`IrqSpinLock`] whenever the same data may be locked from an interrupt handler. Use
//! [`SpinLock`] only for data touched from process context in situations where a handler cannot
//! try to take the same lock (otherwise the handler can deadlock with the interrupted holder).
//!
//! [`IrqSafe`] is the [`LockPolicy`] that selects [`IrqSpinLock`] for
//! generic types such as [`WaitQueue`](crate::sync::WaitQueue).

use core::{
    cell::UnsafeCell,
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    arch::hal::cpu::LocalIrqGuard,
    sync::{Lock, LockPolicy},
};

/// Exclusive spinlock that does **not** mask local interrupts.
///
/// Callers spin with [`core::hint::spin_loop`] until the lock is free. There is no sleeping,
/// fairness, or lock poisoning.
///
/// # Interrupt safety
///
/// Local IRQs remain enabled while the lock is held. Taking this lock from an interrupt handler
/// can deadlock if the interrupted context already holds it. Prefer [`IrqSpinLock`] for shared
/// state that handlers may touch.
///
/// # Memory ordering
///
/// Successful acquisition uses [`Ordering::Acquire`]; release uses [`Ordering::Release`], so
/// critical-section accesses do not reorder outside the lock.
pub struct SpinLock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

/// RAII guard for [`SpinLock`]; releases the lock on drop.
///
/// Derefs to the protected data. Do not `mem::forget` the guard without unlocking — that would
/// leave the spinlock permanently acquired.
pub struct SpinLockGuard<'a, T: 'a> {
    lock: &'a AtomicBool,
    data: *mut T,
}

// SAFETY: `SpinLock` only gives out exclusive references via the guard. `T: Send` is required so
// the protected value can move between harts when the lock moves / is accessed from another hart.
unsafe impl<T: Send> Sync for SpinLock<T> {}
// SAFETY: transferring a `SpinLock<T>` to another hart transfers ownership of `T`.
unsafe impl<T: Send> Send for SpinLock<T> {}

// SAFETY: a shared reference to the guard only yields shared access to `T`.
unsafe impl<T: Sync> Sync for SpinLockGuard<'_, T> {}
// SAFETY: moving the guard to another hart moves exclusive access to `T`.
unsafe impl<T: Send> Send for SpinLockGuard<'_, T> {}

impl<T> SpinLock<T> {
    /// Creates a spinlock protecting `data`.
    #[inline]
    pub const fn new(data: T) -> Self {
        SpinLock {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquires the lock, spinning until it is free.
    ///
    /// Uses a test-and-test-and-set loop: a weak CAS to grab the lock, then relaxed loads while
    /// waiting so failed acquisitions do not constantly bounce the cache line in exclusive state.
    #[inline]
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        loop {
            if let Some(guard) = self.try_lock_weak() {
                return guard;
            }

            while self.is_locked() {
                core::hint::spin_loop();
            }
        }
    }

    /// Returns whether the lock is currently held.
    ///
    /// The result may be stale immediately on SMP; it is only a hint for the spin wait loop.
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.lock.load(Ordering::Relaxed)
    }

    /// Attempts to acquire the lock with a strong compare-exchange.
    ///
    /// Returns [`None`] if the lock is contended.
    /// Prefer [`lock`](Self::lock) unless implementing a custom acquire loop.
    #[inline]
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        // The reason for using a strong compare_exchange is explained here:
        // https://github.com/Amanieu/parking_lot/pull/207#issuecomment-575869107
        if self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinLockGuard {
                lock: &self.lock,
                data: self.data.get(),
            })
        } else {
            None
        }
    }

    /// Attempts to acquire the lock with a weak compare-exchange.
    ///
    /// Returns [`None`] if the lock is contended or if the CAS fails spuriously. Prefer
    /// [`lock`](Self::lock) unless implementing a custom acquire loop.
    #[inline]
    pub fn try_lock_weak(&self) -> Option<SpinLockGuard<'_, T>> {
        if self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinLockGuard {
                lock: &self.lock,
                data: self.data.get(),
            })
        } else {
            None
        }
    }
}

impl<T: Default> Default for SpinLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for SpinLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => write!(f, "SpinLock {{ data: ")
                .and_then(|()| (*guard).fmt(f))
                .and_then(|()| write!(f, " }}")),
            None => write!(f, "SpinLock {{ <locked> }}"),
        }
    }
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: we hold the lock, so we have exclusive access to the data.
        unsafe { &*self.data }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: we hold the lock, so we have exclusive access to the data.
        unsafe { &mut *self.data }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
    }
}

impl<'a, T: fmt::Debug> fmt::Debug for SpinLockGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T> Lock for SpinLock<T> {
    type Target = T;

    type Guard<'a>
        = SpinLockGuard<'a, T>
    where
        Self: 'a;

    fn new(data: Self::Target) -> Self {
        SpinLock::new(data)
    }

    fn lock(&self) -> Self::Guard<'_> {
        self.lock()
    }
}

/// Spinlock that masks local interrupts for the duration of the critical section.
///
/// Acquisition order is: disable IRQs, then lock the inner [`SpinLock`]. Release order is the
/// reverse (unlock, then restore IRQs), enforced by [`IrqSpinLockGuard`] field drop order.
///
/// Nesting is supported: each acquisition saves and restores the previous IRQ-enable state via
/// `LocalIrqGuard`.
///
/// This prevents the classic deadlock where a process holds the lock, an interrupt runs on the
/// same hart, and the handler tries to take the same lock.
pub struct IrqSpinLock<T> {
    inner: SpinLock<T>,
}

/// Guard for [`IrqSpinLock`]; unlocks the inner spinlock and restores local IRQ state on drop.
///
/// Field order is load-bearing: `guard` must be declared before `_irq_guard` so the spinlock is
/// released while interrupts are still disabled.
pub struct IrqSpinLockGuard<'a, T: 'a> {
    // IMPORTANT: keep this field first, so that it is dropped before `_irq_guard`.
    guard: SpinLockGuard<'a, T>,
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
            inner: SpinLock::new(data),
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
/// This is the default policy for [`WaitQueue`](crate::sync::WaitQueue).
pub struct IrqSafe;

impl LockPolicy for IrqSafe {
    type Lock<T> = IrqSpinLock<T>;
}
