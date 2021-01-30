use riscv::{
    instructions::wfi,
    registers::{SiFlags, Sie, Sip, Sstatus, SstatusFlags},
};

mod entry;
mod memory;
mod sbi;
mod stackframe;
mod time;
mod trap;

/// Halts execution on the current hart forever.
pub fn halt() -> ! {
    // Disable all interrupts.
    unsafe { Sstatus::clear(SstatusFlags::SIE) };
    Sie::clear(SiFlags::SSIE | SiFlags::STIE | SiFlags::SEIE);
    Sip::write_raw(0);

    // Loop forever
    loop {
        wfi();
    }
}
