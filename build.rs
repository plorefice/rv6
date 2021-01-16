fn main() {
    // Build startup code and archive it
    cc::Build::new()
        .file("src/cpu/startup.S")
        .file("src/cpu/trap.S")
        .compile("libcpu.a");

    // Trigger a rerun on some external file changes
    println!("cargo:rerun-if-changed=src/cpu/startup.S");
    println!("cargo:rerun-if-changed=src/cpu/trap.S");
    println!("cargo:rerun-if-changed=src/cpu/qemu/qemu-virt.ld");
    println!("cargo:rerun-if-changed=build.rs");
}
