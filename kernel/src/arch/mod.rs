//! Architecture-specific functions.

/// RISC-V architecture.
#[cfg(target_arch = "riscv64")]
mod riscv;

// Re-export the architecture-specific modules.
#[cfg(target_arch = "riscv64")]
pub use self::riscv::*;

/// A struct containing architecture-specific services that can be used by the kernel.
///
/// This struct is intended to be passed around to various parts of the kernel that need to
/// perform architecture-specific operations, such as memory management, I/O mapping, ELF loading.
pub struct ArchServices {
    /// Information on the page layout used by the architecture.
    pub page_layout: ArchPageLayout,
    /// The architecture-specific DMA allocator.
    pub dma: ArchDmaAllocator,
    /// The architecture-specific I/O mapper.
    pub io: ArchIoMapper,
    /// The architecture-specific ELF loader.
    pub loader: ArchLoaderImpl,
    /// The architecture-specific user process executor.
    pub uexec: ArchUserExecutor,
    /// The architecture-specific user memory layout.
    pub uml: ArchUserMemoryLayout,
    /// A function to halt the current hart.
    pub halt: fn() -> !,
}

/// The global instance of the architecture-specific services.
static ARCH: spin::Once<ArchServices> = spin::Once::INIT;

/// Sets the global architecture-specific services.
///
/// This function should be called exactly once during kernel initialization, and the provided
/// services should be valid for the entire lifetime of the kernel.
pub fn set_arch_services(services: ArchServices) {
    ARCH.call_once(|| services);
}

/// Returns a reference to the global architecture-specific services.
pub fn get_arch_services() -> &'static ArchServices {
    // SAFETY: init code must call `set_arch_services` for the kernel to function,
    //         and we don't want to pay the cost of checking this every time
    unsafe { ARCH.get_unchecked() }
}
