mod entry;
mod trap;

macro_rules! csr {
    ($var:ident, $type:ident, $str:tt) => {
        pub struct $type;

        pub static $var: $type = $type;

        impl $type {
            #[inline(always)]
            pub fn modify<F>(&self, f: F)
            where
                F: FnOnce(usize) -> usize,
            {
                self.write(f(self.read()));
            }

            #[inline(always)]
            pub fn read(&self) -> usize {
                let r: usize;
                unsafe { asm!(concat!("csrr {}, ", $str), out(reg) r) };
                r
            }

            #[inline(always)]
            pub fn write(&self, val: usize) {
                unsafe { asm!(concat!("csrw ", concat!($str, ", {}")), in(reg) val) };
            }
        }
    };
}

// Machine CSRs
csr!(MISA, MIsa, "misa");
csr!(MVENDORID, MVendorId, "mvendorid");
csr!(MARCHID, MArchId, "marchid");
csr!(MIMPID, MImpId, "mimpid");
csr!(MHARTID, MHartId, "mhartid");
csr!(MSTATUS, MStatus, "mstatus");
csr!(MTVEC, MtVec, "mtvec");
csr!(MEDELEG, MEDeleg, "medeleg");
csr!(MIDELEG, MIDeleg, "mideleg");
csr!(MIP, Mip, "mip");
csr!(MIE, Mie, "mie");
csr!(MTIME, MTime, "mtime");
csr!(MTIMECMP, MTimeCmp, "mtimecmp");
csr!(MCOUNTEREN, MCounterEn, "mcounteren");
csr!(MCOUNTINHIBIT, MCountInhibit, "mcountinhibit");
csr!(MSCRATCH, MScratch, "mscratch");
csr!(MEPC, Mepc, "mepc");
csr!(MCAUSE, MCause, "mcause");
csr!(MTVAL, MtVal, "mtval");

// Supervisor CSRs
csr!(SSTATUS, SStatus, "sstatus");
csr!(STVEC, StVec, "stvec");
csr!(SIP, Sip, "sip");
csr!(SIE, Sie, "sie");
csr!(SCOUNTEREN, SCounterEn, "scounteren");
csr!(SSCRATCH, SScratch, "sscratch");
csr!(SEPC, Sepc, "sepc");
csr!(SCAUSE, SCause, "scause");
csr!(STVAL, StVal, "stval");
csr!(SATP, Satp, "satp");

/// Halts execution on the current hart.
pub fn abort() -> ! {
    // TODO: this should also disable interrupts.
    loop {
        wfi();
    }
}

/// Blocks hart's execution until an interrupt is pending.
fn wfi() {
    unsafe { asm!("wfi", options(nostack)) };
}
