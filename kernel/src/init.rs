//! Kernel init thread: mount rootfs, load `/init`, spawn userspace init.
//!
//! Runs as the first kernel thread after `kmain` enters the scheduler, so
//! process-context primitives ([`crate::sync::Mutex`], park/wake) are valid here.

use alloc::vec::Vec;
use fdt::Fdt;

use crate::{
    arch::hal,
    block::{BLOCK_DEVS, BlockDevCursor},
    initrd,
    proc::ProcessBuilder,
    vfs::{self, fd::OpenFlags},
};

/// Entry point of the first kernel thread.
///
/// Runs with a `current` process, so process-context primitives (`Mutex`, park/wake) are
/// valid here. Mounts the rootfs, loads `/init` (falling back to the initrd), spawns
/// userspace init and returns; `kthread_trampoline` then calls `kthread_exit` and idle
/// schedules user `/init`.
///
/// `fdt_ptr` is the relocated FDT pointer passed from `kmain` (already validated there).
pub fn kernel_init(fdt_ptr: usize) {
    let init_code = load_init_image(fdt_ptr).expect("no init program found");
    kprintln!("Found init program, size {}", init_code.len());
    hal::proc::builder().spawn_init(init_code);
}

fn load_init_image(fdt_ptr: usize) -> Option<Vec<u8>> {
    let rootfs_init = mount_root_fs().and_then(read_root_init);

    match rootfs_init {
        Some(buf) => {
            kprintln!("Loaded /init from rootfs");
            Some(buf)
        }
        None => {
            kprintln!("Failed to load init from rootfs, falling back to initrd");
            load_initrd_init(fdt_ptr)
        }
    }
}

fn mount_root_fs() -> Option<&'static vfs::ext2::Fs> {
    let blkdev = {
        let table = BLOCK_DEVS.lock();
        table.iter().next().cloned()
    };

    let Some(blkdev) = blkdev else {
        kprintln!("No block device found");
        return None;
    };

    match ext2::FileSystem::mount(BlockDevCursor::new(blkdev)) {
        Ok(fs) => Some(vfs::init_root_fs(vfs::ext2::Fs::new(fs))),
        Err(e) => {
            kprintln!("Failed to mount root filesystem: {e}");
            None
        }
    }
}

fn read_root_init(fs: &vfs::ext2::Fs) -> Option<Vec<u8>> {
    match fs.open("/init", OpenFlags::READ).and_then(|f| {
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(buf)
    }) {
        Ok(buf) => Some(buf),
        Err(e) => {
            kprintln!("Failed to read /init from root filesystem: {e}");
            None
        }
    }
}

fn load_initrd_init(fdt_ptr: usize) -> Option<Vec<u8>> {
    // SAFETY: `fdt_ptr` is the relocated FDT pointer that `kmain` already parsed successfully.
    let fdt = unsafe { Fdt::from_raw_ptr(fdt_ptr as *const u8) }.expect("invalid fdt data");
    let initrd = initrd::load_from_fdt(&fdt).expect("failed to load initrd");
    initrd.find_file("init").map(|f| f.to_vec())
}
