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
        if Base::probe_extension(Extension::Timer).is_ok() {
            kprintln!("SBI v0.2 TIME extension detected");
        }
    } else {
        panic!("Unsupported SBI specification");
    }
}
