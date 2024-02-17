/// Core timebase, expressed in number of cycles per second.
/// TODO: read this from SBI
pub const CLINT_TIMEBASE: u64 = 10_000_000;

/// Returns the number of cycles elapsed since boot, in timebase units.
pub fn get_cycles() -> u64 {
    riscv::registers::Time::read()
}

/// Schedules a timer interrupt to happend `interval` ticks in the future.
pub fn schedule_next_tick(interval: u64) {
    sbi::timer::set_timer(get_cycles() + interval).unwrap();
}
