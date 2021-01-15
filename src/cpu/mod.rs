mod trap;

pub use trap::*;

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

// Supervisor CSRs
csr!(SSTATUS, Sstatus, "sstatus");
csr!(STVEC, Stvec, "stvec");
csr!(SIP, Sip, "sip");
csr!(SIE, Sie, "sie");
csr!(SCOUNTEREN, Scounteren, "scounteren");
csr!(SSCRATCH, Sscratch, "sscratch");
csr!(SEPC, Sepc, "sepc");
csr!(SCAUSE, Scause, "scause");
csr!(STVAL, Stval, "stval");
csr!(SATP, Satp, "satp");
