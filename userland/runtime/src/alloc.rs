use core::{
    alloc::{AllocError, Allocator, GlobalAllocator, Layout},
    num::NonZero,
    ptr::NonNull,
};

use crate::syscall::sys_sbrk;

#[global_allocator]
static GLOBAL: SystemAllocator = SystemAllocator::new();

struct SystemAllocator {
    inner: spin::Mutex<SystemAllocatorInner>,
}

impl SystemAllocator {
    const fn new() -> Self {
        Self {
            inner: spin::Mutex::new(SystemAllocatorInner::new()),
        }
    }
}

struct SystemAllocatorInner {
    brk: Option<NonZero<usize>>,
}

impl SystemAllocatorInner {
    const fn new() -> Self {
        Self { brk: None }
    }
}

unsafe impl Allocator for SystemAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let mut inner = self.inner.lock();
        let brk = match inner.brk {
            Some(brk) => brk.get(),
            None => {
                // SAFETY: we are not modifying the program break, just reading it.
                let brk = unsafe { sys_sbrk(0) }.map_err(|_| AllocError)?;
                inner.brk = Some(brk);
                brk.get()
            }
        };
        let aligned_brk = brk.next_multiple_of(layout.align());
        let next_brk = aligned_brk + layout.size();
        let increment = next_brk - brk;

        // SAFETY: increment is valid and we have a lock on the inner state, so no other thread can
        //         modify the program break while we are adjusting it via a safe API
        unsafe { sys_sbrk(increment as isize) }.map_err(|_| AllocError)?;
        inner.brk = Some(NonZero::new(next_brk).ok_or(AllocError)?);

        let ptr = aligned_brk as *mut u8;
        Ok(NonNull::slice_from_raw_parts(
            NonNull::new(ptr).ok_or(AllocError)?,
            layout.size(),
        ))
    }

    unsafe fn deallocate(&self, _: NonNull<u8>, _: Layout) {
        // TODO: implement deallocation by adjusting the program break downwards if possible
    }
}

unsafe impl GlobalAllocator for SystemAllocator {}
