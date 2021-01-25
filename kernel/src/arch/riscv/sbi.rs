use sbi::{Base, Extension};

pub fn init() {
    let version = Base::get_spec_version();

    kprintln!(
        "SBI specification v{}.{} detected",
        version.major,
        version.minor,
    );

    if version.minor != 0x1 {
        kprintln!(
            "SBI implementation ID=0x{:x} Version=0x{:x}",
            Base::get_impl_id().unwrap(),
            Base::get_impl_version().unwrap(),
        );
        kprint!("SBI v0.2 detected extensions:");
        if Base::probe_extension(Extension::Timer).is_ok() {
            kprint!(" TIMER");
        }
        if Base::probe_extension(Extension::Ipi).is_ok() {
            kprint!(" IPI");
        }
        if Base::probe_extension(Extension::Rfence).is_ok() {
            kprint!(" RFENCE");
        }
        if Base::probe_extension(Extension::Hsm).is_ok() {
            kprint!(" HSM");
        }
        if Base::probe_extension(Extension::SystemReset).is_ok() {
            kprint!(" SYSRST");
        }
        kprintln!();
    } else {
        panic!("Unsupported SBI specification");
    }
}
