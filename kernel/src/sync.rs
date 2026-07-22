//! Synchronization primitives for the kernel.

use core::ops::{Deref, DerefMut};

use alloc::collections::VecDeque;
use spin::{Mutex, MutexGuard};

use crate::{
    proc::{ProcessId, ProcessState, global_process_table},
    sched,
};

mod spinlock;

pub use spinlock::*;

/// A trait for locking mechanisms that provide mutually exclusive access to data.
pub trait Lock {
    /// The type of data protected by the lock.
    type Target: ?Sized;

    /// The type of guard returned by the lock, which provides access to the protected data.
    type Guard<'a>: Deref<Target = Self::Target> + DerefMut + 'a
    where
        Self: 'a;

    /// Creates a new instance of the lock, initializing it with the provided data.
    fn new(data: Self::Target) -> Self
    where
        Self::Target: Sized;

    /// Locks the data, returning a guard that provides access to it.
    ///
    /// Blocking behavior and other details depend on the specific implementation of the lock.
    fn lock(&self) -> Self::Guard<'_>;
}

/// The underlying lock strategy used by a lock.
pub trait LockPolicy {
    /// The type of lock that implements this policy.
    type Lock<T: ?Sized>: Lock<Target = T> + ?Sized;
}

impl<T: ?Sized> Lock for Mutex<T> {
    type Target = T;

    type Guard<'a>
        = MutexGuard<'a, T>
    where
        Self: 'a;

    fn new(data: Self::Target) -> Self
    where
        Self::Target: Sized,
    {
        Mutex::new(data)
    }

    fn lock(&self) -> Self::Guard<'_> {
        self.lock()
    }
}

/// A lock policy for process-context-only that uses a `spin::Mutex` for synchronization.
///
/// It can be used to protect data that is accessed by multiple processes, but does not require
/// disabling interrupts. As such it must not be used to protect resources that are accessed from
/// interrupt context.
pub struct ProcessContext;

impl LockPolicy for ProcessContext {
    type Lock<T: ?Sized> = Mutex<T>;
}

/// A wait queue for processes with associated data.
///
/// This structure allows processes to wait for certain events or conditions to be met before
/// they can continue execution. Processes can be added to the wait queue and will be woken up when
/// the event they are waiting for occurs.
pub struct WaitQueue<T> {
    data: Mutex<T>,
    sleepers: Mutex<VecDeque<ProcessId>>,
}

impl<T: Default> Default for WaitQueue<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> WaitQueue<T> {
    /// Creates a new wait queue with the given data.
    pub const fn new(data: T) -> Self {
        WaitQueue {
            data: Mutex::new(data),
            sleepers: Mutex::new(VecDeque::new()),
        }
    }

    /// Locks the wait queue's data and returns a guard that allows access to it.
    ///
    /// This function is useful for accessing or modifying the data associated with the wait queue.
    pub fn lock(&self) -> WaitQueueGuard<'_, T> {
        WaitQueueGuard {
            wq: self,
            inner: self.data.lock(),
        }
    }

    /// Waits until the provided condition is met, blocking the current process if necessary.
    pub fn wait_until(&self, mut ready: impl FnMut(&T) -> bool) -> WaitQueueGuard<'_, T> {
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

    fn arm(&self, pid: ProcessId) {
        let mut pt = global_process_table().lock();
        let proc = pt.get_mut(pid).expect("arm: invalid process ID");
        debug_assert!(
            matches!(proc.state, ProcessState::Running),
            "arm: expected Running"
        );
        proc.state = ProcessState::Waiting;
        self.sleepers.lock().push_back(pid);
    }
}

/// A guard that provides access to the data in a wait queue while ensuring proper synchronization.
pub struct WaitQueueGuard<'a, T> {
    wq: &'a WaitQueue<T>,
    inner: MutexGuard<'a, T>,
}

impl<'a, T> WaitQueueGuard<'a, T> {
    /// Wakes up one process from the wait queue, if any.
    pub fn wake_one(&self) -> Option<ProcessId> {
        let pid = self.wq.sleepers.lock().pop_front()?;
        sched::wake_process(pid);
        Some(pid)
    }

    /// Wakes up all processes in the wait queue.
    pub fn wake_all(&self) {
        while self.wake_one().is_some() {}
    }
}

impl<T> Deref for WaitQueueGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for WaitQueueGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
