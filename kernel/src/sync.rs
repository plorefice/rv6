//! Kernel synchronization primitives.
//!
//! This module provides:
//!
//! - [`Lock`] — exclusive-lock interface shared by concrete lock types
//! - [`LockPolicy`] — selects which lock implementation a generic type (e.g. [`WaitQueue`]) uses
//! - [`WaitQueue`] — condition-style wait/wake over associated data
//! - [`SpinLock`] — plain spinning lock (not IRQ-safe)
//! - [`IrqSpinLock`] / [`IrqSafe`] — IRQ-safe spinning locks
//!
//! # Choosing a lock policy
//!
//! | Policy | Concrete lock | Disables local IRQs? | Safe from IRQ handlers? |
//! |--------|---------------|----------------------|-------------------------|
//! | [`IrqSafe`] (default for [`WaitQueue`]) | [`IrqSpinLock`] | yes | yes |
//! | [`ProcessContext`] | [`SpinLock`] | no | **no** |
//!
//! Both policies still **spin** while the lock is contended; neither puts the caller to sleep.
//! [`ProcessContext`] only means “process context only” — it is not a sleeping mutex.
//!
//! Use [`IrqSafe`] whenever the same data may be locked from an interrupt handler (typical for
//! wait queues woken by device IRQs). Use [`ProcessContext`] only for data touched exclusively
//! from process context where a handler cannot take the same lock.

use core::ops::{Deref, DerefMut};

use alloc::collections::VecDeque;

use crate::{
    proc::{ProcessId, ProcessState, global_process_table},
    sched,
};

mod spinlock;

pub use spinlock::*;

/// Exclusive access to a value of type [`Target`](Lock::Target).
///
/// Implementations differ in interrupt safety and whether they disable local IRQs while held
/// (see [`LockPolicy`]). All current implementations spin on contention.
pub trait Lock {
    /// Type of the value protected by this lock.
    type Target;

    /// RAII guard returned by [`lock`](Lock::lock); unlocks on drop.
    type Guard<'a>: Deref<Target = Self::Target> + DerefMut + 'a
    where
        Self: 'a;

    /// Creates a lock protecting `data`.
    fn new(data: Self::Target) -> Self;

    /// Acquires the lock and returns a guard for the protected data.
    ///
    /// Spins until the lock is free. Whether local interrupts are masked for the critical
    /// section depends on the concrete lock type.
    fn lock(&self) -> Self::Guard<'_>;
}

/// Selects the concrete [`Lock`] type used by a generic synchronized structure.
///
/// Policies are zero-sized markers. The associated type constructor [`Lock`](LockPolicy::Lock)
/// builds a lock around any payload `T`, so types like [`WaitQueue`] can protect several fields
/// with the same strategy:
///
/// ```ignore
/// struct Example<P: LockPolicy> {
///     a: P::Lock<u32>,
///     b: P::Lock<VecDeque<u8>>,
/// }
/// ```
///
/// See the [module-level overview](crate::sync) and the [`IrqSafe`] / [`ProcessContext`] docs.
pub trait LockPolicy {
    /// Lock type this policy uses for a payload `T`.
    type Lock<T>: Lock<Target = T>;
}

/// [`LockPolicy`] for data accessed only from process context.
///
/// Uses [`SpinLock`]: callers spin on contention and **local interrupts stay enabled**.
/// Taking this lock from an interrupt handler can deadlock if the interrupted context already
/// holds it.
///
/// Prefer [`IrqSafe`] when in doubt, especially for wait queues.
pub struct ProcessContext;

impl LockPolicy for ProcessContext {
    type Lock<T> = SpinLock<T>;
}

/// Wait queue pairing protected data with a list of sleeping processes.
///
/// Processes block with [`wait_until`](WaitQueue::wait_until) until a predicate over the data
/// holds; wakers update the data under [`lock`](WaitQueue::lock) and call
/// [`wake_one`](WaitQueueGuard::wake_one) / [`wake_all`](WaitQueueGuard::wake_all).
///
/// The lock policy `P` applies to both the payload and the sleeper list. The default is
/// [`IrqSafe`], which is appropriate when waking from interrupt handlers (e.g. a UART RX
/// queue). Use [`WaitQueue<T, ProcessContext>`] only if every lock and wake stays in process
/// context.
///
/// # Lost wakeups
///
/// [`wait_until`](WaitQueue::wait_until) arms the process, drops the lock, then parks via
/// [`sched::park_armed`]. An early wake before the park is handled by that API.
pub struct WaitQueue<T, P: LockPolicy = IrqSafe> {
    data: P::Lock<T>,
    sleepers: P::Lock<VecDeque<ProcessId>>,
}

impl<T: Default> Default for WaitQueue<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T, P: LockPolicy> WaitQueue<T, P> {
    /// Creates a wait queue with the given initial data and an empty sleeper list.
    pub fn new(data: T) -> Self {
        WaitQueue {
            data: P::Lock::new(data),
            sleepers: P::Lock::new(VecDeque::new()),
        }
    }

    /// Locks the wait-queue data and returns a guard.
    ///
    /// Use the guard to mutate the payload and to [`wake_one`](WaitQueueGuard::wake_one) /
    /// [`wake_all`](WaitQueueGuard::wake_all) sleepers.
    pub fn lock(&self) -> WaitQueueGuard<'_, T, P> {
        WaitQueueGuard {
            wq: self,
            inner: self.data.lock(),
        }
    }

    /// Blocks the current process until `ready` is true for the protected data.
    ///
    /// Returns a guard with the data still locked so the caller can consume or update it.
    ///
    /// # Panics
    ///
    /// Panics if there is no current process (must not be called from the idle context).
    pub fn wait_until(&self, mut ready: impl FnMut(&T) -> bool) -> WaitQueueGuard<'_, T, P> {
        let pid = sched::current_process_id().expect("wait_until: no current process");

        let mut guard = self.lock();
        while !ready(&*guard) {
            self.arm(pid);
            drop(guard);
            sched::park_armed();
            guard = self.lock();
        }
        guard
    }

    /// Marks `pid` waiting and enqueues it on this wait queue.
    ///
    /// Called while the data lock is held; the caller must drop that lock before parking.
    fn arm(&self, pid: ProcessId) {
        let mut sleepers = self.sleepers.lock();
        let mut pt = global_process_table().lock();
        let proc = pt.get_mut(pid).expect("arm: invalid process ID");
        debug_assert!(
            matches!(proc.state, ProcessState::Running),
            "arm: expected Running"
        );
        proc.state = ProcessState::Waiting;
        sleepers.push_back(pid);
    }
}

/// Guard for [`WaitQueue`] data; unlocks on drop.
///
/// Derefs to the protected payload. Wake methods may be called while the guard is held.
pub struct WaitQueueGuard<'a, T, P: LockPolicy> {
    wq: &'a WaitQueue<T, P>,
    inner: <<P as LockPolicy>::Lock<T> as Lock>::Guard<'a>,
}

impl<'a, T, P: LockPolicy> WaitQueueGuard<'a, T, P> {
    /// Dequeues one sleeper and marks it runnable, if any.
    ///
    /// Returns the woken process id, or [`None`] if the sleeper list was empty.
    pub fn wake_one(&self) -> Option<ProcessId> {
        let pid = self.wq.sleepers.lock().pop_front()?;
        sched::wake_process(pid);
        Some(pid)
    }

    /// Wakes every process currently queued on this wait queue.
    pub fn wake_all(&self) {
        while self.wake_one().is_some() {}
    }
}

impl<T, P: LockPolicy> Deref for WaitQueueGuard<'_, T, P> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, P: LockPolicy> DerefMut for WaitQueueGuard<'_, T, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
