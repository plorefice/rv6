use riscv::registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags};

mod entry;
mod memory;
mod sbi;
mod time;
mod trap;

/// Halts execution on the current hart forever.
///
/// # Safety
///
/// Unsafe low-level machine operation.
pub unsafe fn halt() -> ! {
    // Disable all interrupts.
    Sstatus::clear(SstatusFlags::SIE);
    Sie::clear(SiFlags::SSIE | SiFlags::STIE | SiFlags::SEIE);
    Sip::write_raw(0);

    // Loop forever
    loop {
        asm!("wfi", options(nostack));
    }
}
