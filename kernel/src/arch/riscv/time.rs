//! RISC-V timekeeping.

use core::time::Duration;

use crate::arch::riscv::{registers::Time, sbi};

/// Core timebase, expressed in number of cycles per second.
/// TODO: read this from SBI
pub const CLINT_TIMEBASE: u64 = 10_000_000;

/// Returns the number of cycles elapsed since boot, in timebase units.
pub fn get_cycles() -> u64 {
    Time::read()
}

/// Schedules a timer interrupt to happend `interval` ticks in the future.
pub fn schedule_next_tick(d: Duration) {
    sbi::timer::set_timer(get_cycles() + duration_to_ticks(d)).unwrap();
}

fn duration_to_ticks(d: Duration) -> u64 {
    let secs = d.as_secs();
    let nanos = d.subsec_nanos() as u64;

    secs * CLINT_TIMEBASE + nanos * CLINT_TIMEBASE / 1_000_000_000
}
