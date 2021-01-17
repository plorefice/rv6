use super::*;

extern "C" {
    // Defined in trap.S
    fn _strap_entry();
}

#[no_mangle]
#[link_section = ".trap.rust"]
#[export_name = "_do_handle_strap_rust"]
pub extern "C" fn do_handle_strap_rust() {
    let cause = SCAUSE.read();

    println!(
        r#"
=> Trap handler!
    sstatus: {:016x}
     scause: {:016x}
      stval: {:016x}
        sip: {:016x}
        sie: {:016x}
       sepc: {:016x}
"#,
        SSTATUS.read(),
        cause,
        STVAL.read(),
        SIP.read(),
        SIE.read(),
        SEPC.read()
    );

    // Breakpoint: skip the EBREAK instruction
    if cause == 0x3 {
        SEPC.modify(|pc| pc + 4);
    }
}

/// Configures the trap vector used to handle traps in S-mode.
pub fn init_trap_vector() {
    STVEC.write(_strap_entry as *const () as usize);

    // Enable interrupts
    SIE.modify(|r| r | (1 << 9) | (1 << 5) | (1 << 1));
}
