use core::sync::atomic::{AtomicBool, Ordering};

use crate::sync::WaitQueue;

/// Synchronization primitive to signal when a certain task has been completed.
pub struct Completion {
    done: AtomicBool,
    wq: WaitQueue<()>,
}

impl Default for Completion {
    fn default() -> Self {
        Self::new()
    }
}

impl Completion {
    /// Creates a new [`Completion`] object in the "not done" state.
    pub fn new() -> Self {
        Completion {
            done: AtomicBool::new(false),
            wq: WaitQueue::new(()),
        }
    }

    /// Waits for the task to be completed.
    ///
    /// This method waits for the completion of a task; it is not interruptible and there is no timeout.
    pub fn wait(&self) {
        self.wq.wait_until(|_| self.done.load(Ordering::Acquire));
    }

    /// Marks the task as completed and wakes up any waiting threads.
    ///
    /// This method sets the completion state to "done" and wakes up any threads that are waiting
    /// for the task to be completed. Any subsequent calls to [`Completion::wait`] will return immediately
    /// without blocking unless [`Completion::reset`] is called to reset the completion state.
    pub fn complete(&self) {
        self.done.store(true, Ordering::Release);
        let guard = self.wq.lock();
        guard.wake_all();
    }

    /// Resets the completion state to "not done".
    ///
    /// After calling this method, subsequent calls to [`Completion::wait`] will block until
    /// [`Completion::complete`] is called again.
    pub fn reset(&self) {
        self.done.store(false, Ordering::Release);
    }
}
