use super::*;

extern "C" {
    // Defined in trap.S
    fn trap_entry();
}

#[no_mangle]
#[link_section = ".trap.rust"]
pub extern "C" fn handle_exception(cause: usize, epc: usize, tval: usize, _regp: usize) -> usize {
    println!(
        "=> Exception: cause {:016x}, epc {:016x}, tval {:016x}",
        cause, epc, tval,
    );

    // Breakpoint: skip the EBREAK instruction
    if cause == 0x3 {
        epc + 4
    } else {
        epc
    }
}

/// Configures the trap vector used to handle traps in S-mode.
pub fn init_trap_vector() {
    STVEC.write(trap_entry as *const () as usize);

    // Enable interrupts
    SIE.modify(|r| r | (1 << 9) | (1 << 5) | (1 << 1));
}
