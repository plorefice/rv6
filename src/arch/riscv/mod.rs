mod entry;
mod memory;
mod sbi;
mod time;
mod trap;

// {m,s}ie register flags
const IE_SSIE: usize = 1 << 1;
const IE_STIE: usize = 1 << 5;
const IE_SEIE: usize = 1 << 9;

// {m,s}status register flags
const STATUS_SIE: usize = 1 << 1;

macro_rules! csr {
    ($var:ident, $type:ident, $str:tt) => {
        /// Wrapper type for this CSR.
        pub struct $type;

        /// Global instance of this CSR for this hart.
        pub static $var: $type = $type;

        impl $type {
            /// Performs a read-modify-write operation on this register.
            #[inline(always)]
            pub fn modify<F>(&self, f: F)
            where
                F: FnOnce(usize) -> usize,
            {
                self.write(f(self.read()));
            }

            /// Reads the content of this register.
            #[inline(always)]
            pub fn read(&self) -> usize {
                let r: usize;
                unsafe { asm!(concat!("csrr {}, ", $str), out(reg) r) };
                r
            }

            /// Writes a value to this register.
            #[inline(always)]
            pub fn write(&self, val: usize) {
                unsafe { asm!(concat!("csrw ", concat!($str, ", {}")), in(reg) val) };
            }

            /// Sets the bits specified in `mask` on this register.
            #[inline(always)]
            pub fn set(&self, mask: usize) {
                unsafe { asm!(concat!("csrs ", concat!($str, ", {}")), in(reg) mask) };
            }

            /// Clears the bits specified in `mask` on this register.
            #[inline(always)]
            pub fn clear(&self, mask: usize) {
                unsafe { asm!(concat!("csrc ", concat!($str, ", {}")), in(reg) mask) };
            }
        }
    };
}

// Supervisor CSRs
csr!(SSTATUS, SStatus, "sstatus");
csr!(STVEC, StVec, "stvec");
csr!(SIP, Sip, "sip");
csr!(SIE, Sie, "sie");
csr!(STVAL, StVal, "stval");
csr!(SATP, Satp, "satp");

/// Halts execution on the current hart forever.
///
/// # Safety
///
/// Unsafe low-level machine operation.
pub unsafe fn halt() -> ! {
    // Disable all interrupts.
    SSTATUS.clear(STATUS_SIE);
    SIE.clear(IE_SSIE | IE_STIE | IE_SEIE);
    SIP.write(0);

    // Loop forever
    loop {
        asm!("wfi", options(nostack));
    }
}
