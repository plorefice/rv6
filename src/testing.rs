/// Utility trait for automagically printing a test function name before execution.
pub trait Testable {
    /// Runs the test function.
    fn run(&self);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        kprint!("test {} ... ", core::any::type_name::<T>());
        self();
        kprintln!("ok");
    }
}

/// Entry point for test harness.
#[cfg(test)]
pub fn test_runner(tests: &[&dyn Testable]) {
    use crate::drivers::syscon::SYSCON;

    kprintln!("Running {} tests", tests.len());

    for test in tests {
        test.run();
    }

    // Exit from QEMU correctly
    SYSCON.lock().poweroff(0);
}
