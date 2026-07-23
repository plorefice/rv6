//! Sleeping mutex.
//!
//! [`Mutex`] provides exclusive access and **parks** the caller on contention instead of spinning.
//! It is built on a [`WaitQueue`] that tracks the locked flag; the protected value lives in an
//! [`UnsafeCell`] so the wait-queue spinlock is not held across the critical section.
//!
//! # Process context only
//!
//! Must be used from process context: [`lock`](Mutex::lock) parks via the scheduler and panics if
//! there is no current process. Never take a [`Mutex`] from an interrupt handler.

use core::{
    cell::UnsafeCell,
    fmt,
    ops::{Deref, DerefMut},
};

use crate::sync::{Lock, ProcessContext, WaitQueue};

/// Exclusive sleeping lock.
///
/// Contended callers block with [`WaitQueue::wait_until`] until the lock is free, then mark it
/// held and return a [`MutexGuard`]. There is no fairness or lock poisoning.
///
/// The wait queue uses [`ProcessContext`] (plain [`SpinLock`](crate::sync::SpinLock) metadata):
/// local interrupts stay enabled while updating the locked flag. The mutex itself must not be
/// acquired from IRQ handlers.
///
/// # Panics
///
/// [`lock`](Self::lock) panics if there is no current process.
pub struct Mutex<T> {
    locked: WaitQueue<bool, ProcessContext>,
    data: UnsafeCell<T>,
}

/// RAII guard for [`Mutex`]; releases the lock and wakes one waiter on drop.
///
/// Derefs to the protected data. Do not `mem::forget` the guard without unlocking — that would
/// leave the mutex permanently acquired.
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
    data: *mut T,
}

// SAFETY: `Mutex` only gives out exclusive references via the guard. `T: Send` is required so
// the protected value can move between harts when the lock moves / is accessed from another hart.
unsafe impl<T: Send> Sync for Mutex<T> {}
// SAFETY: transferring a `Mutex<T>` to another hart transfers ownership of `T`.
unsafe impl<T: Send> Send for Mutex<T> {}

// SAFETY: a shared reference to the guard only yields shared access to `T`.
unsafe impl<T: Sync> Sync for MutexGuard<'_, T> {}
// SAFETY: moving the guard to another hart moves exclusive access to `T`.
unsafe impl<T: Send> Send for MutexGuard<'_, T> {}

impl<T> Mutex<T> {
    /// Creates a mutex protecting `data`.
    #[inline]
    pub fn new(data: T) -> Self {
        Self {
            locked: WaitQueue::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquires the lock, sleeping until it is free.
    ///
    /// # Panics
    ///
    /// Panics if there is no current process (must not be called from the idle context).
    #[inline]
    pub fn lock(&self) -> MutexGuard<'_, T> {
        let mut g = self.locked.wait_until(|l| !*l);
        *g = true;
        drop(g);

        MutexGuard {
            mutex: self,
            data: self.data.get(),
        }
    }

    /// Attempts to acquire the lock without sleeping.
    ///
    /// Returns [`None`] if the mutex is already held. May briefly spin on the wait-queue
    /// metadata lock while inspecting the locked flag.
    #[inline]
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        let mut g = self.locked.lock();
        if *g {
            return None;
        }
        *g = true;
        drop(g);

        Some(MutexGuard {
            mutex: self,
            data: self.data.get(),
        })
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => write!(f, "Mutex {{ data: ")
                .and_then(|()| (*guard).fmt(f))
                .and_then(|()| write!(f, " }}")),
            None => write!(f, "Mutex {{ <locked> }}"),
        }
    }
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: we hold the lock, so we have exclusive access to the data.
        unsafe { &*self.data }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: we hold the lock, so we have exclusive access to the data.
        unsafe { &mut *self.data }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        let mut g = self.mutex.locked.lock();
        *g = false;
        g.wake_one();
    }
}

impl<'a, T: fmt::Debug> fmt::Debug for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T> Lock for Mutex<T> {
    type Target = T;

    type Guard<'a>
        = MutexGuard<'a, T>
    where
        Self: 'a;

    fn new(data: Self::Target) -> Self {
        Self::new(data)
    }

    fn lock(&self) -> Self::Guard<'_> {
        self.lock()
    }
}
