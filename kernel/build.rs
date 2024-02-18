use std::env;

fn main() {
    let target = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    if target == "riscv64" {
        // Build startup code and archive it
        let mut cc = cc::Build::new();
        cc.compiler("riscv64-elf-gcc").flag("-mabi=lp64d");

        #[cfg(feature = "config-qemu")]
        cc.flag("-DCONFIG_QEMU");
        #[cfg(feature = "config-milkv")]
        cc.flag("-DCONFIG_MILKV");

        cc.file("src/arch/riscv/startup.S")
            .file("src/arch/riscv/trap.S")
            .compile("libcpu.a");

        println!("cargo:rerun-if-changed=src/arch/riscv/startup.S");
        println!("cargo:rerun-if-changed=src/arch/riscv/trap.S");
        println!("cargo:rerun-if-changed=linkers/riscv.ld");
    }

    println!("cargo:rerun-if-changed=build.rs");
}
