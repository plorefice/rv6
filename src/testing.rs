/// Entry point for test harness.
#[cfg(test)]
pub fn test_runner(tests: &[&dyn Fn()]) {
    use crate::drivers::syscon::SYSCON;

    kprintln!("Running {} tests", tests.len());

    for test in tests {
        test();
    }

    // Exit from QEMU correctly
    SYSCON.lock().poweroff(0);
}
