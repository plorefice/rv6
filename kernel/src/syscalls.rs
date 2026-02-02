//! Syscalls implementation.

use crate::arch;

/// A raw pointer to a user-space memory location.
///
/// User memory cannot be directly dereferenced from kernel space, so this type is used to
/// represent pointers to user memory safely. To access the data pointed to by a `UserPtr`,
/// functions like `copy_from_user` must be used.
#[derive(Debug, Clone, Copy)]
pub struct UserPtr<T> {
    addr: usize,
    _marker: core::marker::PhantomData<T>,
}

impl<T> UserPtr<T> {
    /// Marks a raw user-space pointer.
    pub fn new(addr: usize) -> Self {
        UserPtr {
            addr,
            _marker: core::marker::PhantomData,
        }
    }
}

/// Copies `dst.len()` bytes from the user-space buffer `src` to the kernel-space buffer `dst`.
///
/// # Safety
///
/// This function assumes that `src` is a valid user-space pointer and that the memory region
/// it points to is accessible. The caller must ensure these conditions are met.
pub unsafe fn copy_from_user(dst: &mut [u8], src: UserPtr<u8>) {
    arch::with_user_access(|| unsafe {
        // SAFETY: TODO: validate user pointer
        let mut p = src.addr as *const u8;
        for b in dst.iter_mut() {
            *b = core::ptr::read_volatile(p);
            p = p.add(1);
        }
    });
}

/// Writes `len` bytes from the user-space buffer `buf` to the specified file descriptor.
///
/// # Note
///
/// For simplicity, this implementation only supports writing to `fd=1` (stdout).
pub fn sys_write(fd: usize, buf: UserPtr<u8>, len: usize) -> isize {
    // For simplicity, only support fd=1 (stdout)
    if fd != 1 {
        return -1;
    }

    let mut slice = vec![0u8; len];
    // SAFETY: TODO: validate user pointer
    unsafe { copy_from_user(&mut slice[..], buf) };

    match core::str::from_utf8(slice.as_slice()) {
        Ok(s) => {
            kprint!("{}", s); // TODO: use console driver
            len as isize
        }
        Err(_) => -1,
    }
}
