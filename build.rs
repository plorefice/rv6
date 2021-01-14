fn main() {
    // Build startup code and archive it
    cc::Build::new()
        .file("src/arch/startup.S")
        .compile("libstartup.a");

    // Trigger a rerun on some external file changes
    println!("cargo:rerun-if-changed=src/arch/startup.S");
    println!("cargo:rerun-if-changed=src/scripts/qemu-virt.ld");
    println!("cargo:rerun-if-changed=build.rs");
}
