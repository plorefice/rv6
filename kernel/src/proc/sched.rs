//! Process sheduling.

use alloc::{boxed::Box, collections::VecDeque};
use spin::Mutex;

use crate::proc::{PROCESS_TABLE, Process, ProcessId};

/// The scheduler trait, which defines the interface for scheduling processes in the system.
pub trait Scheduler: Send + Sync {
    /// Schedules the next process to run and returns its context.
    fn schedule(&mut self) -> Option<ProcessId>;

    /// Enqueues a process to be scheduled.
    fn enqueue(&mut self, proc_id: ProcessId);

    /// Removes the current process from the scheduler, typically called when a process exits.
    fn exit_current(&mut self, pid: ProcessId);

    /// Returns the currently running process, if any.
    fn current(&self) -> Option<ProcessId>;
}

/// A simple round-robin scheduler implementation.
/// This scheduler maintains a queue of runnable processes and cycles through them in order.
#[derive(Debug, Default)]
pub struct RoundRobinScheduler {
    current: Option<ProcessId>,
    run_queue: VecDeque<ProcessId>,
}

impl Scheduler for RoundRobinScheduler {
    fn schedule(&mut self) -> Option<ProcessId> {
        if let Some(current) = self.current.take() {
            self.run_queue.push_back(current);
        }

        self.current = self.run_queue.pop_front();
        self.current
    }

    fn enqueue(&mut self, proc_id: ProcessId) {
        self.run_queue.push_back(proc_id);
    }

    fn exit_current(&mut self, pid: ProcessId) {
        self.run_queue.retain(|&p| p != pid);
        if self.current == Some(pid) {
            self.current = None;
        }
    }

    fn current(&self) -> Option<ProcessId> {
        self.current
    }
}

// Global scheduler instance, protected by a mutex for safe concurrent access.
static SCHEDULER: Mutex<Option<Box<dyn Scheduler>>> = Mutex::new(None);

/// Initializes the scheduler subsystem with the given scheduler implementation.
pub fn init(sched: Box<dyn Scheduler>) {
    *SCHEDULER.lock() = Some(sched);
}

/// Runs the scheduler to pick the next process to run and performs a context switch if necessary.
pub fn run_scheduler() {
    let next = {
        let mut sched = SCHEDULER.lock();
        let sched = match sched.as_mut() {
            Some(sched) => sched,
            None => return,
        };

        let current = sched.current();
        let next = match sched.schedule() {
            Some(next) => next,
            None => return,
        };

        if current == Some(next) {
            return;
        }

        next
    };

    crate::arch::hal::proc::resume(next);
}

/// Allocates a new process in the process table and returns its unique identifier.
pub fn allocate_process(p: Process) -> ProcessId {
    let mut table = PROCESS_TABLE.lock();
    table.allocate(p)
}

/// Enqueues the specified process to be scheduled by the scheduler.
pub fn enqueue_process(proc_id: ProcessId) {
    let mut sched = SCHEDULER.lock();
    let sched = sched.as_mut().expect("scheduler not initialized");

    // Enqueue the process to be scheduled
    sched.enqueue(proc_id);

    // If there's no current process, run the scheduler to pick one
    if sched.current().is_none() {
        sched.schedule();
    }
}

/// Removes the specified process from the scheduler, typically called when a process exits.
pub fn exit_current(pid: ProcessId) {
    let mut sched = SCHEDULER.lock();
    let sched = sched.as_mut().expect("scheduler not initialized");

    sched.exit_current(pid);
}

/// Returns the identifier of the currently running process, if any.
pub fn current_process_id() -> Option<ProcessId> {
    SCHEDULER.lock().as_ref()?.current()
}
