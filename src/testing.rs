/// Entry point for test harness.
#[cfg(test)]
pub fn test_runner(tests: &[&dyn Fn()]) {
    kprintln!("Running {} tests", tests.len());

    for test in tests {
        test();
    }
}
