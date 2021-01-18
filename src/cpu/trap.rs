use super::*;

// {m,s}cause register flags
const CAUSE_IRQ_FLAG_MASK: usize = 1 << 63;

// {m,s}ie register flags
const IE_SSIE: usize = 1 << 1;
const IE_STIE: usize = 1 << 5;
const IE_SEIE: usize = 1 << 9;

// {m,s}status register flags
const STATUS_SIE: usize = 1 << 1;

#[repr(usize)]
enum IrqCause {
    STimer = 5,
}

#[no_mangle]
#[link_section = ".trap.rust"]
pub extern "C" fn handle_exception(cause: usize, epc: usize, tval: usize, _regp: usize) -> usize {
    let is_irq = (cause & CAUSE_IRQ_FLAG_MASK) != 0;
    let irq = cause & !CAUSE_IRQ_FLAG_MASK;

    if is_irq {
        if irq == IrqCause::STimer as usize {
            println!("Tick!");
            time::schedule_next_tick(time::CLINT_TIMEBASE);
        }
    } else {
        panic!(
            "=> Exception: cause {:016x}, epc {:016x}, tval {:016x}",
            cause, epc, tval,
        );
    }

    epc
}

/// Configures the trap vector used to handle traps in S-mode.
pub fn init() {
    extern "C" {
        // Defined in trap.S
        fn trap_entry();
    }

    STVEC.write(trap_entry as *const () as usize);

    // Enable interrupts
    SIE.set(IE_SSIE | IE_STIE | IE_SEIE);
    SSTATUS.set(STATUS_SIE);
}
