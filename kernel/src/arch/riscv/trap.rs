//! RISC-V exception handling.

use stackframe::unwind_stack_frame;

use crate::arch::riscv::registers::Stvec;

use super::*;

// {m,s}cause register flags
const CAUSE_IRQ_FLAG_MASK: usize = 1 << 63;

/// Possible interrupt causes on a RISC-V CPU.
#[repr(usize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum IrqCause {
    STimer = 5,
}

/// Possible exception causes on a RISC-V CPU.
#[repr(usize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum ExceptionCause {
    InstrAddrMisaligned,
    InstrAccessFault,
    IllegalInstr,
    Breakpoint,
    LoadAddrMisaligned,
    LoadAccessFault,
    StoreAddrMisaligned,
    StoreAccessFault,
    EnvCallFromU,
    EnvCallFromS,
    InstrPageFault,
    LoadPageFault,
    StorePageFault,
}

impl From<usize> for ExceptionCause {
    fn from(n: usize) -> Self {
        use ExceptionCause::*;

        match n {
            0 => InstrAddrMisaligned,
            1 => InstrAccessFault,
            2 => IllegalInstr,
            3 => Breakpoint,
            4 => LoadAddrMisaligned,
            5 => LoadAccessFault,
            6 => StoreAddrMisaligned,
            7 => StoreAccessFault,
            8 => EnvCallFromU,
            9 => EnvCallFromS,
            12 => InstrPageFault,
            13 => LoadPageFault,
            15 => StorePageFault,
            _ => panic!("invalid exception cause: {}", n),
        }
    }
}

/// Information stored by the trap handler.
///
/// Note: the order of the fields in this structure **must** match the order in which registers
/// are pushed to the stack in the handler's trampoline.
#[repr(C)]
struct TrapFrame {
    ra: usize,
    sp: usize,
    gp: usize,
    tp: usize,
    t0: usize,
    t1: usize,
    t2: usize,
    s0: usize,
    s1: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
    a7: usize,
    s2: usize,
    s3: usize,
    s4: usize,
    s5: usize,
    s6: usize,
    s7: usize,
    s8: usize,
    s9: usize,
    s10: usize,
    s11: usize,
    t3: usize,
    t4: usize,
    t5: usize,
    t6: usize,
}

impl TrapFrame {
    /// Prints the content of the trap frame to the console.
    #[rustfmt::skip]
    fn dump(&self, pc: usize) {
        let s = self;
        kprintln!(" PC was at {:016x}", pc);
        kprintln!(" RA was at {:016x}", s.ra);
        kprintln!(" sp : {:016x}  gp : {:016x}  tp : {:016x}", s.sp, s.gp, s.tp);
        kprintln!(" t0 : {:016x}  t1 : {:016x}  t2 : {:016x}", s.t0, s.t1, s.t2);
        kprintln!(" s0 : {:016x}  s1 : {:016x}  a0 : {:016x}", s.s0, s.s1, s.a0);
        kprintln!(" a1 : {:016x}  a2 : {:016x}  a3 : {:016x}", s.a1, s.a2, s.a3);
        kprintln!(" a4 : {:016x}  a5 : {:016x}  a6 : {:016x}", s.a4, s.a5, s.a6);
        kprintln!(" a7 : {:016x}  s2 : {:016x}  s3 : {:016x}", s.a7, s.s2, s.s3);
        kprintln!(" s4 : {:016x}  s5 : {:016x}  s6 : {:016x}", s.s4, s.s5, s.s6);
        kprintln!(" s7 : {:016x}  s8 : {:016x}  s9 : {:016x}", s.s7, s.s8, s.s9);
        kprintln!(" s10: {:016x}  s11: {:016x}  t3 : {:016x}", s.s10, s.s11, s.t3);
        kprintln!(" t4 : {:016x}  t5 : {:016x}  t6 : {:016x}", s.t4, s.t5, s.t6);
    }
}

#[no_mangle]
extern "C" fn handle_exception(cause: usize, epc: usize, tval: usize, tf: &TrapFrame) -> usize {
    let is_irq = (cause & CAUSE_IRQ_FLAG_MASK) != 0;
    let irq = cause & !CAUSE_IRQ_FLAG_MASK;

    if is_irq {
        if irq == IrqCause::STimer as usize {
            kprintln!("Tick!");
            time::schedule_next_tick(time::CLINT_TIMEBASE);
        }

        // After an interrupt, continue from where we left off
        epc
    } else {
        use ExceptionCause::*;

        match ExceptionCause::from(irq) {
            InstrPageFault | LoadPageFault | StorePageFault => {
                kprintln!("=> Page fault trying to access {:016x}", tval)
            }
            ex => kprintln!("=> Unhandled exception: {:?}, tval {:016x}", ex, tval),
        }

        // Debug facilities
        tf.dump(epc);
        unwind_stack_frame();

        // Halt the hart. This will change when exceptions are handled.
        halt();
    }
}

/// Configures the trap vector used to handle traps in S-mode.
pub fn init() {
    extern "C" {
        // Defined in trap.S
        fn trap_entry();
    }

    // Configure trap vector to point to `trap_entry`
    Stvec::write(trap_entry as *const () as u64);

    // Enable interrupts
    Sie::set(SiFlags::SSIE | SiFlags::STIE | SiFlags::SEIE);
    // SAFETY: stvec has been initialized to point to `trap_entry`
    unsafe { Sstatus::set(SstatusFlags::SIE) };
}
