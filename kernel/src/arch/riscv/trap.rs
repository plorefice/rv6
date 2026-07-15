//! RISC-V exception handling.

use core::arch::asm;
use core::time::Duration;

use stackframe::unwind_stack_frame;

use crate::{
    arch::riscv::{
        mmu::dump_active_root_page_table,
        proc::ThreadInfo,
        registers::{Sscratch, Stvec},
    },
    proc, sched,
    syscall::{self, Errno, SysArgs, SysResult, UserPtr},
};

use super::*;

// {m,s}cause register flags
const CAUSE_IRQ_FLAG_MASK: usize = 1 << 63;

/// Possible interrupt causes on a RISC-V CPU.
#[repr(usize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum IrqCause {
    Software,
    Timer,
    External,
}

impl From<usize> for IrqCause {
    fn from(n: usize) -> Self {
        use IrqCause::*;

        match n {
            1 => Software,
            5 => Timer,
            9 => External,
            _ => unreachable!(),
        }
    }
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
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TrapFrame {
    pub ra: usize,
    pub sp: usize,
    pub gp: usize,
    pub tp: usize,
    pub t0: usize,
    pub t1: usize,
    pub t2: usize,
    pub s0: usize,
    pub s1: usize,
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
    pub a6: usize,
    pub a7: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
    pub t3: usize,
    pub t4: usize,
    pub t5: usize,
    pub t6: usize,
    pub status: usize,
    pub epc: usize,
    pub tval: usize,
    pub cause: usize,
}

// Sanity checks to ensure that the layout of `TrapFrame` matches the expectations of the assembly code in `trap.S`.
const _: () = assert!(core::mem::size_of::<TrapFrame>() == 280);
const _: () = assert!(core::mem::offset_of!(TrapFrame, epc) == 256);

impl From<&TrapFrame> for SysArgs {
    fn from(value: &TrapFrame) -> Self {
        let TrapFrame {
            a0,
            a1,
            a2,
            a3,
            a4,
            a5,
            a6,
            ..
        } = value;

        SysArgs::new([*a0, *a1, *a2, *a3, *a4, *a5])
    }
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

#[unsafe(no_mangle)]
extern "C" fn handle_exception(tf: &mut TrapFrame, ti: &ThreadInfo) {
    // Invariant: when handling a trap, we are always in kernel mode, so sscratch should be 0
    debug_assert!(Sscratch::read() == 0);

    let mut gpt = proc::global_process_table().lock();
    if let Some(pid) = sched::current_process_id() {
        let proc = gpt.get_mut(pid).expect("current process doesn't exist");
        proc.astate.tf.clone_from(tf);
        proc.astate.ti.clone_from(ti);
    }
    drop(gpt);

    let is_irq = (tf.cause & CAUSE_IRQ_FLAG_MASK) != 0;
    let irq = tf.cause & !CAUSE_IRQ_FLAG_MASK;

    if is_irq {
        let irq = IrqCause::from(irq);

        match irq {
            IrqCause::Timer => {
                time::schedule_next_tick(Duration::from_millis(25));
                sched::run_scheduler();
            }
            _ => kprintln!("Unhandled IRQ: {:?}", irq),
        }
    } else {
        use ExceptionCause::*;

        match ExceptionCause::from(irq) {
            InstrPageFault | LoadPageFault | StorePageFault => {
                let kind = match ExceptionCause::from(irq) {
                    InstrPageFault => "Instruction fetch",
                    LoadPageFault => "Load",
                    StorePageFault => "Store",
                    _ => unreachable!(),
                };
                kprintln!("=> {} page fault trying to access {:016x}", kind, tf.tval)
            }
            EnvCallFromU => {
                handle_syscall(tf);
                tf.epc += 4;
                return;
            }
            ex => kprintln!("=> Unhandled exception: {:?}, tval {:016x}", ex, tf.tval),
        }

        // Debug facilities
        tf.dump(tf.epc);
        dump_active_root_page_table();
        unwind_stack_frame();

        // Halt the hart. This will change when exceptions are handled.
        halt();
    }
}

fn handle_syscall(tf: &mut TrapFrame) {
    let sysno = tf.a7;
    let args = SysArgs::from(&*tf);

    let res = match sysno {
        x if x == syscall::Sysno::Write as usize => syscall::sys_write(args),
        x if x == syscall::Sysno::Exit as usize => syscall::sys_exit(args),
        x if x == syscall::Sysno::Fork as usize => syscall::sys_fork(args),
        x if x == syscall::Sysno::Wait as usize => syscall::sys_wait(args),
        n => {
            kprintln!("=> Unknown syscall number: {}", n);
            Err(Errno::ENOSYS)
        }
    };

    tf.a0 = syscall::to_ret(res);
}

/// Configures the trap vector used to handle traps in S-mode.
pub fn init(hart_id: usize) {
    unsafe extern "C" {
        // Defined in trap.S
        fn trap_entry();
    }

    // Prepare kernel thread info.
    // This must be done before enabling interrupts, since trap code expects to find
    // a valid pointer to a thread info struct in TP.
    init_kernel_thread_info(hart_id);

    // Configure trap vector to point to `trap_entry`
    Stvec::write(trap_entry as *const () as u64);

    // Enable interrupts
    Sie::set(SiFlags::SSIE | SiFlags::STIE | SiFlags::SEIE);
    // SAFETY: stvec has been initialized to point to `trap_entry`
    unsafe { Sstatus::set(SstatusFlags::SIE) };
}

/// Allocates and initializes a thread info struct for the kernel thread running on the current hart,
/// and writes its address to TP.
fn init_kernel_thread_info(hart_id: usize) {
    let ksl = mm::kstack_layout(hart_id);

    // Place a thread info struct at the top of the kernel stack
    let kti_ptr = ksl.start.as_mut_ptr::<ThreadInfo>();

    // SAFETY: kti_ptr is valid by design
    unsafe {
        kti_ptr.write_volatile(ThreadInfo {
            ksp: ksl.initial_sp.as_usize(),
            usp: 0, // not used for this idle kernel thread
        })
    };

    // Write this address in tp
    // SAFETY: in kernel space, tp is reserved for this use
    unsafe {
        asm!("mv tp, {}", in(reg) kti_ptr);
    }
}
