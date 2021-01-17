// Default SBI version is 0.1
const SPEC_VERSION_DEFAULT: usize = 0x1;

#[repr(usize)]
enum Extension {
    Base = 0x10,
    Time = 0x54494D45,
}

#[repr(usize)]
enum ExtBaseFid {
    SpecVersion = 0,
    ImpID,
    ImpVersion,
    ProbeExt,
}

type Result = core::result::Result<usize, isize>;

#[allow(clippy::too_many_arguments)]
fn do_ecall(
    ext: usize,
    fid: usize,
    mut a0: usize,
    mut a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> Result {
    unsafe {
        asm!("ecall",
                inout("a0") a0,
                inout("a1") a1,
                in("a2") a2,
                in("a3") a3,
                in("a4") a4,
                in("a5") a5,
                in("a6") fid,
                in("a7") ext);
    }

    if a0 == 0 {
        Ok(a1)
    } else {
        Err(a0 as isize)
    }
}

fn base_ecall(fid: ExtBaseFid) -> Result {
    do_ecall(Extension::Base as usize, fid as usize, 0, 0, 0, 0, 0, 0)
}

fn get_spec_version() -> Result {
    base_ecall(ExtBaseFid::SpecVersion)
}

fn get_firmware_id() -> usize {
    base_ecall(ExtBaseFid::ImpID).unwrap()
}

fn get_firmware_version() -> usize {
    base_ecall(ExtBaseFid::ImpVersion).unwrap()
}

fn probe_extension(ext: Extension) -> Result {
    do_ecall(
        Extension::Base as usize,
        ExtBaseFid::ProbeExt as usize,
        ext as usize,
        0,
        0,
        0,
        0,
        0,
    )
}

fn get_major(version: usize) -> usize {
    (version >> 24) & 0x7f
}

fn get_minor(version: usize) -> usize {
    version & 0xffffff
}

pub fn init() {
    let version = get_spec_version().unwrap_or(SPEC_VERSION_DEFAULT);

    println!(
        "SBI specification v{}.{} detected",
        get_major(version),
        get_minor(version)
    );

    if version != SPEC_VERSION_DEFAULT {
        println!(
            "SBI implementation ID=0x{:x} Version=0x{:x}",
            get_firmware_id(),
            get_firmware_version(),
        );
        if probe_extension(Extension::Time).is_ok() {
            println!("SBI v0.2 TIME extension detected");
        }
    } else {
        panic!("Unsupported SBI specification");
    }
}
