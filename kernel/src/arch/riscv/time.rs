use crate::mm::mmio::read_volatile;

const CLINT_BASE: usize = 0x0200_0000;
const CLINT_TIME_OFFSET: usize = 0xbff8;

/// Core timebase, expressed in number of cycles per second.
pub const CLINT_TIMEBASE: u64 = 10_000_000;

/// Returns the number of cycles elapsed since boot, in timebase units.
pub fn get_cycles() -> u64 {
    read_volatile(CLINT_BASE, CLINT_TIME_OFFSET)
}

/// Schedules a timer interrupt to happend `interval` ticks in the future.
pub fn schedule_next_tick(interval: u64) {
    sbi::Timer::set_timer(get_cycles() + interval).unwrap();
}
