//! Process scheduling.

use alloc::{boxed::Box, collections::VecDeque};
use spin::Mutex;

use crate::{
    arch::hal,
    drivers::syscon,
    proc::{PROCESS_TABLE, Process, ProcessId, ProcessState},
};

/// The scheduler trait, which defines the interface for scheduling processes in the system.
pub trait Scheduler: Send + Sync {
    /// Schedules the next process to run and returns its id.
    ///
    /// If there is a current process, it is moved to the back of the run queue first
    /// (cooperative yield). Call [`Scheduler::exit_current`] before this when the
    /// outgoing process must not be requeued (park / exit).
    fn schedule(&mut self) -> Option<ProcessId>;

    /// Enqueues a process to be scheduled.
    fn enqueue(&mut self, proc_id: ProcessId);

    /// Removes the process from the run queue and clears it as current if needed.
    fn exit_current(&mut self, pid: ProcessId);

    /// Returns the currently running process, if any.
    fn current(&self) -> Option<ProcessId>;

    /// Sets the current process without touching the run queue.
    fn set_current(&mut self, pid: Option<ProcessId>);
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

    fn set_current(&mut self, pid: Option<ProcessId>) {
        self.current = pid;
    }
}

// Global scheduler instance, protected by a mutex for safe concurrent access.
static SCHEDULER: Mutex<Option<Box<dyn Scheduler>>> = Mutex::new(None);

/// Initializes the scheduler subsystem with the given scheduler implementation.
pub fn init(sched: Box<dyn Scheduler>) {
    *SCHEDULER.lock() = Some(sched);
}

/// Pops the next runnable process and makes it current.
///
/// Used by the idle loop. Requires `current` to be `None` (idle is not a process).
pub fn take_next() -> Option<ProcessId> {
    let mut sched = SCHEDULER.lock();
    let sched = sched.as_mut().expect("scheduler not initialized");
    debug_assert!(sched.current().is_none());
    sched.schedule()
}

/// Relinquish the CPU and switch to another runnable process.
/// Has no effect when called from the idle task or if current process is the only runnable one.
///
/// Always uses kernel context switch. Userspace entry is via the process's
/// `return_to_user` trampoline (or the normal trap epilogue after preemption).
pub fn yield_cpu() {
    let (outgoing, next) = {
        let mut sched = SCHEDULER.lock();
        let sched = match sched.as_mut() {
            Some(sched) => sched,
            None => return,
        };

        let Some(outgoing) = sched.current() else {
            // No current process (idle). Let the idle loop call `take_next` to pick a new one.
            return;
        };
        let Some(next) = sched.schedule() else {
            // No runnable process (and no current). Caller stays put; idle will wfi.
            return;
        };

        if outgoing == next {
            return;
        }

        (outgoing, next)
    };

    {
        let pt = PROCESS_TABLE.lock();
        let proc = pt.get(next).expect("scheduled process not found");
        assert!(matches!(proc.state, ProcessState::Running));
    }

    hal::proc::switch(Some(outgoing), Some(next));
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
    sched.enqueue(proc_id);
}

/// Parks the current process until it is woken and scheduled again.
///
/// Marks the process [`ProcessState::Waiting`], removes it from the run queue,
/// then switches to another runnable process (or the idle context until one appears).
/// Returns when some other task wakes this process and switches back to it.
pub fn park_current() {
    let pid = current_process_id().expect("no current process");

    {
        let mut pt = PROCESS_TABLE.lock();
        let p = pt.get_mut(pid).expect("current process missing");
        assert!(
            matches!(p.state, ProcessState::Running),
            "park_current: expected Running"
        );
        p.state = ProcessState::Waiting;
    }

    park_process(pid);
}

/// Same as [`park_current`], but expects the current process to have been marked
/// [`ProcessState::Waiting`] already.
///
/// Note that we do not enforce that the process is actually marked as waiting, as another process
/// could have woken it before we got to switch away. In that case, the inner `park_process` will
/// detect that the process is already running due to a lost wakeup or early wake and will resume
/// execution immediately.
pub fn park_armed() {
    let pid = current_process_id().expect("no current process");
    park_process(pid);
}

fn park_process(pid: ProcessId) {
    // Must not be requeued by schedule().
    exit_current(pid);

    // Lost-wakeup / early-wake: already Running again before we switched away.
    {
        let pt = PROCESS_TABLE.lock();
        let p = pt.get(pid).expect("parked process missing");
        if matches!(p.state, ProcessState::Running) {
            let mut sched = SCHEDULER.lock();
            let sched = sched.as_mut().expect("scheduler not initialized");
            sched.set_current(Some(pid));
            return;
        }
    }

    let next = {
        let mut sched = SCHEDULER.lock();
        let sched = sched.as_mut().expect("scheduler not initialized");
        debug_assert!(sched.current().is_none());
        // current is None, so this only pops the run queue.
        sched.schedule()
    };

    match next {
        Some(next_pid) => {
            {
                let pt = PROCESS_TABLE.lock();
                let proc = pt.get(next_pid).expect("next process missing");
                assert!(matches!(proc.state, ProcessState::Running));
            }
            // Returns when someone switches back to us.
            hal::proc::switch(Some(pid), Some(next_pid));
        }
        None => {
            // No other runnable process — switch to idle until woken.
            hal::proc::switch(Some(pid), None);
        }
    }
}

/// Marks a waiting process runnable and places it on the run queue.
///
/// Does not change which process is current, so a waker does not steal `current`
/// from a task that is still on the CPU in [`park_current`].
pub fn wake_process(proc_id: ProcessId) {
    {
        let mut pt = PROCESS_TABLE.lock();
        let p = pt.get_mut(proc_id).expect("wake_process: invalid pid");
        assert!(
            matches!(p.state, ProcessState::Waiting),
            "wake_process: expected Waiting"
        );
        p.state = ProcessState::Running;
    }
    enqueue_process(proc_id);
}

/// Removes the specified process from the scheduler.
pub fn exit_current(pid: ProcessId) {
    let mut sched = SCHEDULER.lock();
    let sched = sched.as_mut().expect("scheduler not initialized");
    sched.exit_current(pid);
}

/// Picks the next runnable process after `pid` has left the scheduler, then switches to it.
///
/// Used by process exit so a parked waiter can resume via `switch`. If nothing remains
/// runnable, powers off.
pub fn switch_from_exiting(pid: ProcessId) -> ! {
    let next = {
        let mut sched = SCHEDULER.lock();
        let sched = sched.as_mut().expect("scheduler not initialized");
        debug_assert!(sched.current() != Some(pid));
        sched.schedule()
    };

    match next {
        Some(next_pid) => {
            {
                let pt = PROCESS_TABLE.lock();
                let proc = pt.get(next_pid).expect("next process missing");
                assert!(matches!(proc.state, ProcessState::Running));
            }
            hal::proc::switch(Some(pid), Some(next_pid));
            kprintln!("switch_from_exiting: resumed exiting process");
            hal::cpu::halt();
        }
        None => {
            kprintln!("All processes have exited. Bye!");
            syscon::poweroff();
            hal::cpu::halt();
        }
    }
}

/// Returns the identifier of the currently running process, if any.
pub fn current_process_id() -> Option<ProcessId> {
    SCHEDULER.lock().as_ref()?.current()
}
