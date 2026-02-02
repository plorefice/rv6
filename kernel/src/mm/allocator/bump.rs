use core::{
    alloc::{GlobalAlloc, Layout},
    marker::PhantomData,
    ptr::null_mut,
};

use spin::Mutex;

use crate::{
    arch::phys_to_virt,
    mm::{
        PhysicalAddress,
        allocator::{Frame, FrameAllocator},
    },
};

/// A simple bump allocator.
///
/// A bump allocator behaves as a stack that can only grow. At each allocation, a new chunk
/// is reserved, starting at the end of the previously allocated chunk.
///
/// Because of this, a bump allocator is extremely fast, with each allocation resulting in just
/// a handful of arithmetic operations and checks.
/// However, this comes at the significant drawback of only being able to perform deallocations
/// only when all allocated memory has been released, in which case the allocator is reset.
#[derive(Debug)]
pub struct BumpAllocator {
    inner: Mutex<BumpImpl>,
}

#[derive(Debug)]
struct BumpImpl {
    start: usize,
    end: usize,
    ptr: usize,
    allocated: usize,
}

impl BumpAllocator {
    /// Creates a new bump allocator.
    ///
    /// # Panics
    ///
    /// Panics if `start > end`.
    pub const fn new(start: usize, end: usize) -> Self {
        assert!(start <= end);
        Self {
            inner: Mutex::new(BumpImpl {
                start,
                end,
                ptr: end,
                allocated: 0,
            }),
        }
    }
}

// SAFETY: BumpAllocator is a global allocator
unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut bump = self.inner.lock();

        let ptr = match bump.ptr.checked_sub(layout.size()) {
            Some(ptr) => ptr,
            None => return null_mut(),
        };

        let ptr = ptr & !(layout.align() - 1);
        if ptr < bump.start {
            return null_mut();
        }

        bump.allocated += 1;
        bump.ptr = ptr;
        bump.ptr as *mut u8
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {
        let mut bump = self.inner.lock();

        bump.allocated -= 1;
        if bump.allocated == 0 {
            bump.ptr = bump.end;
        }
    }
}

/// A bump allocator for physical memory. Deallocation is not supported.
pub struct BumpFrameAllocator<const N: u64, A> {
    inner: BumpImpl,
    _marker: PhantomData<A>,
}

impl<const N: u64, A> BumpFrameAllocator<N, A>
where
    A: PhysicalAddress<u64>,
{
    /// Creates a new bump allocator.
    ///
    /// # Safety
    ///
    /// `start` and `end` must be contained in a valid allocatable memory region. In particular,
    /// the region within must be covered by `pa_to_va`.
    ///
    /// # Panics
    ///
    /// Panics if `start > end`.
    pub unsafe fn new(start: A, end: A) -> Self {
        assert!(start <= end);
        Self {
            inner: BumpImpl {
                start: start.into() as usize,
                end: end.into() as usize,
                ptr: start.into() as usize,
                allocated: 0,
            },
            _marker: Default::default(),
        }
    }
}

impl<const N: u64, A> FrameAllocator<A, N> for BumpFrameAllocator<N, A>
where
    A: PhysicalAddress<u64>,
{
    unsafe fn alloc(&mut self, count: usize) -> Option<Frame<A>> {
        let bump = &mut self.inner;

        let next = bump.ptr.checked_add(count * N as usize)?;
        if next > bump.end {
            return None;
        }

        let paddr = A::try_from(bump.ptr as u64).ok()?;
        let frame = Frame {
            // SAFETY: safe as long as Self::new was called with physical addresses
            ptr: unsafe { phys_to_virt(paddr).as_mut_ptr() },
            paddr,
        };

        bump.ptr = next;
        bump.allocated += count;

        Some(frame)
    }

    unsafe fn free(&mut self, _address: A) {}
}
