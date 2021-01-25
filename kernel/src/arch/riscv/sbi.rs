use sbi::{base, Extension};

pub fn init() {
    let version = base::get_spec_version();

    kprintln!(
        "SBI specification v{}.{} detected",
        version.major,
        version.minor,
    );

    if version.minor != 0x1 {
        kprintln!(
            "SBI implementation ID=0x{:x} Version=0x{:x}",
            base::get_impl_id().unwrap(),
            base::get_impl_version().unwrap(),
        );
        kprint!("SBI v0.2 detected extensions:");
        if base::probe_extension(Extension::Timer).is_ok() {
            kprint!(" TIMER");
        }
        if base::probe_extension(Extension::Ipi).is_ok() {
            kprint!(" IPI");
        }
        if base::probe_extension(Extension::Rfence).is_ok() {
            kprint!(" RFENCE");
        }
        if base::probe_extension(Extension::Hsm).is_ok() {
            kprint!(" HSM");
        }
        if base::probe_extension(Extension::SystemReset).is_ok() {
            kprint!(" SYSRST");
        }
        kprintln!();
    } else {
        panic!("Unsupported SBI specification");
    }
}
