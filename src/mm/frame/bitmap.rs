//! A simple bitmap allocator for physical pages.
//!
//! The allocator keeps track of free and allocated pages by keeping a list of page descriptors
//! at the top of the managed memory, hence the term "bitmap".
//!
//! Each page can either be marked as free, as a part of a larger chunk of allocated memory or
//! as the last (or only) page in a chunk. Discriminating between these last two states allows to
//! free a memory chunk by knowing only its pointer and not its size.
//!
//! # Complexity
//!
//! Freeing a page in a bitmap allocator has `O(1)` complexity, but allocation is more expensive
//! (`O(n)`) since we need to find a large-enough chunk of free pages.

use core::mem::size_of;

use bitflags::bitflags;

use crate::mm::page::Address;

use super::{AllocatorError, FrameAllocator, PhysicalAddress};

bitflags! {
    /// Allocation status of a page.
    struct PageFlags: u8 {
        const TAKEN = 1 << 0;
        const LAST  = 1 << 1;
    }
}

/// A descriptor for a physical memory page.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PageDescriptor {
    flags: PageFlags,
}

/// A frame allocator storing page state as a bitmap of page descriptors.
#[derive(Debug)]
pub struct BitmapAllocator {
    descriptors: *mut PageDescriptor,
    base_addr: PhysicalAddress,
    page_size: usize,
    num_pages: usize,
}

impl BitmapAllocator {
    /// Creates a new bitmap allocator taking ownership of the memory delimited by addresses
    /// `start` and `end`, and allocating pages of size `page_size`.
    ///
    /// Returns an `AllocationError` if any of the following conditions are not met:
    ///  - `start` and `end` are page-aligned,
    ///  - `page_size` is a non-zero power of two.
    ///
    /// # Safety
    ///
    /// There can be no guarantee that the memory being passed to the allocator isn't already in
    /// use by the system, so tread carefully here.
    pub unsafe fn init(
        start: PhysicalAddress,
        end: PhysicalAddress,
        page_size: usize,
    ) -> Result<Self, AllocatorError> {
        if page_size == 0 || !page_size.is_power_of_two() {
            return Err(AllocatorError::InvalidPageSize);
        }
        if !start.is_page_aligned(page_size) || !end.is_page_aligned(page_size) {
            return Err(AllocatorError::UnalignedAddress);
        }

        let total_mem_size = usize::from(end - start);
        let total_num_pages = total_mem_size / page_size;

        // A portion of memory starting from `start` will be reserved to hold page descriptors.
        // Memory available for allocation starts after this reserved memory.
        let reserved_mem = total_num_pages * size_of::<PageDescriptor>();
        let avail_mem_start = (start + reserved_mem).align_to_next_page(page_size);
        let avail_mem_size = end - avail_mem_start;
        let avail_pages = usize::from(avail_mem_size) / page_size;

        // Initially mark all pages as free
        let descriptors = usize::from(start) as *mut PageDescriptor;
        for i in 0..avail_pages {
            descriptors.add(i).write(PageDescriptor {
                flags: PageFlags::empty(),
            });
        }

        Ok(Self {
            descriptors,
            base_addr: avail_mem_start,
            page_size,
            num_pages: avail_pages,
        })
    }
}

impl FrameAllocator for BitmapAllocator {
    unsafe fn alloc(&mut self, count: usize) -> Option<PhysicalAddress> {
        let mut i = 0;

        'outer: while i < self.num_pages {
            let descr = self.descriptors.add(i);

            // Page already taken => keep going.
            if (*descr)
                .flags
                .intersects(PageFlags::TAKEN | PageFlags::LAST)
            {
                i += 1;
                continue;
            }

            // Not enough pages left => abort.
            if self.num_pages - i < count {
                return None;
            }

            // Check if enough contiguous pages are free
            // NOTE: `x` here to make clippy happy.
            let x = i;
            for j in x..x + count {
                let descr = self.descriptors.add(j);

                if (*descr)
                    .flags
                    .intersects(PageFlags::TAKEN | PageFlags::LAST)
                {
                    i = j;
                    continue 'outer;
                }
            }

            // If we get here, we managed to find `count` free pages.
            for j in i..i + count {
                (*self.descriptors.add(j)).flags |= if j == i + count - 1 {
                    PageFlags::LAST
                } else {
                    PageFlags::TAKEN
                };
            }

            return Some(self.base_addr + i * self.page_size);
        }

        None
    }

    unsafe fn free(&mut self, address: PhysicalAddress) {
        let offset = usize::from(address - self.base_addr) / self.page_size;

        for i in offset..self.num_pages {
            let descr = self.descriptors.add(i);
            let flags = &mut (*descr).flags;

            let is_last = flags.contains(PageFlags::LAST);

            // Sanity check
            if *flags == PageFlags::empty() {
                panic!("Trying to free an unallocated page!");
            }

            // Clear page status
            *flags = PageFlags::empty();

            if is_last {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        alloc::{self, Layout},
        ptr,
    };

    use crate::mm::page::PAGE_LENGTH;

    const NUM_PAGES: usize = 32;
    const MEM_SIZE: usize = NUM_PAGES * PAGE_LENGTH;

    #[derive(Debug)]
    struct MemChunk {
        base: *mut u8,
        layout: Layout,
    }

    impl MemChunk {
        fn alloc(size: usize, align: usize) -> Self {
            unsafe {
                let layout = Layout::from_size_align(size, align).unwrap();
                let base = alloc::alloc(layout);
                ptr::write_bytes(base, 0xAA, size);
                Self { base, layout }
            }
        }
    }

    impl Drop for MemChunk {
        fn drop(&mut self) {
            unsafe { alloc::dealloc(self.base, self.layout) };
        }
    }

    #[test]
    fn construction() {
        let memory = MemChunk::alloc(MEM_SIZE, PAGE_LENGTH);

        let allocator = unsafe {
            BitmapAllocator::init(
                PhysicalAddress::new(memory.base as usize),
                PhysicalAddress::new(memory.base as usize) + MEM_SIZE,
                PAGE_LENGTH,
            )
            .unwrap()
        };

        assert_eq!(allocator.page_size, PAGE_LENGTH);
        assert_eq!(allocator.num_pages, NUM_PAGES - 1);
        assert_eq!(allocator.descriptors as *mut u8, memory.base);
        assert_eq!(
            allocator.base_addr,
            PhysicalAddress::new(memory.base as usize) + PAGE_LENGTH,
        );

        for i in 0..NUM_PAGES - 1 {
            assert_eq!(
                unsafe { allocator.descriptors.add(i).read() },
                PageDescriptor {
                    flags: PageFlags::empty()
                }
            );
        }
    }

    #[test]
    fn invalid_addresses() {
        for t in &[
            (1, PAGE_LENGTH),
            (PAGE_LENGTH, 2 * PAGE_LENGTH - 1),
            (1, PAGE_LENGTH - 1),
        ] {
            unsafe {
                assert!(matches!(
                    BitmapAllocator::init(
                        PhysicalAddress::new(t.0),
                        PhysicalAddress::new(t.1),
                        PAGE_LENGTH,
                    ),
                    Err(AllocatorError::UnalignedAddress)
                ));
            }
        }
    }

    #[test]
    fn invalid_page_size() {
        for t in &[0, 3, 24, PAGE_LENGTH - 1, PAGE_LENGTH + 2] {
            unsafe {
                assert!(matches!(
                    BitmapAllocator::init(
                        PhysicalAddress::new(0),
                        PhysicalAddress::new(PAGE_LENGTH),
                        *t,
                    ),
                    Err(AllocatorError::InvalidPageSize)
                ));
            }
        }
    }

    #[test]
    fn single_page() {
        let memory = MemChunk::alloc(MEM_SIZE, PAGE_LENGTH);

        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(memory.base as usize),
                PhysicalAddress::new(memory.base as usize) + MEM_SIZE,
                PAGE_LENGTH,
            )
            .unwrap();

            let ptr = allocator.alloc(1).expect("allocation failed");
            assert_allocated(&mut allocator, 0, 1);

            allocator.free(ptr);
            assert_free(&mut allocator, 0, 1);
        };
    }

    #[test]
    fn multiple_pages() {
        let memory = MemChunk::alloc(MEM_SIZE, PAGE_LENGTH);

        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(memory.base as usize),
                PhysicalAddress::new(memory.base as usize) + MEM_SIZE,
                PAGE_LENGTH,
            )
            .unwrap();

            let ptr = allocator.alloc(4).expect("allocation failed");
            assert_allocated(&mut allocator, 0, 4);

            allocator.free(ptr);
            assert_free(&mut allocator, 0, 4);
        };
    }

    #[test]
    fn multiple_allocations() {
        let memory = MemChunk::alloc(MEM_SIZE, PAGE_LENGTH);

        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(memory.base as usize),
                PhysicalAddress::new(memory.base as usize) + MEM_SIZE,
                PAGE_LENGTH,
            )
            .unwrap();

            let p1 = allocator.alloc(4).expect("allocation #1 failed");
            let p2 = allocator.alloc(1).expect("allocation #2 failed");
            let p3 = allocator.alloc(3).expect("allocation #3 failed");

            assert_allocated(&mut allocator, 0, 4);
            assert_allocated(&mut allocator, 4, 1);
            assert_allocated(&mut allocator, 5, 3);

            allocator.free(p1);
            assert_free(&mut allocator, 0, 4);
            assert_allocated(&mut allocator, 4, 1);
            assert_allocated(&mut allocator, 5, 3);

            allocator.free(p3);
            assert_free(&mut allocator, 0, 4);
            assert_allocated(&mut allocator, 4, 1);
            assert_free(&mut allocator, 5, 3);

            allocator.free(p2);
            assert_free(&mut allocator, 0, NUM_PAGES - 1);
        };
    }

    #[test]
    fn reuse_pages() {
        let memory = MemChunk::alloc(MEM_SIZE, PAGE_LENGTH);

        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(memory.base as usize),
                PhysicalAddress::new(memory.base as usize) + MEM_SIZE,
                PAGE_LENGTH,
            )
            .unwrap();

            let p1 = allocator.alloc(4).expect("allocation #1 failed");
            let p2 = allocator.alloc(2).expect("allocation #2 failed");

            allocator.free(p1);

            let p1 = allocator.alloc(2).expect("re-allocation failed");

            assert_allocated(&mut allocator, 0, 2);
            assert_free(&mut allocator, 2, 2);
            assert_allocated(&mut allocator, 4, 2);

            allocator.free(p1);
            allocator.free(p2);

            assert_free(&mut allocator, 0, NUM_PAGES - 1);
        };
    }

    #[test]
    fn big_allocation() {
        let memory = MemChunk::alloc(MEM_SIZE, PAGE_LENGTH);

        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(memory.base as usize),
                PhysicalAddress::new(memory.base as usize) + MEM_SIZE,
                PAGE_LENGTH,
            )
            .unwrap();

            allocator
                .alloc(NUM_PAGES)
                .expect_none("big allocation succeeded");

            allocator
                .alloc(2 * NUM_PAGES)
                .expect_none("big allocation succeeded");

            allocator.alloc(1).expect("allocation failed");
        };
    }

    #[test]
    fn spare_allocation() {
        let memory = MemChunk::alloc(MEM_SIZE, PAGE_LENGTH);

        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(memory.base as usize),
                PhysicalAddress::new(memory.base as usize) + MEM_SIZE,
                PAGE_LENGTH,
            )
            .unwrap();

            let _ = allocator.alloc((NUM_PAGES - 1) / 3).unwrap();
            let p = allocator.alloc((NUM_PAGES - 1) / 3).unwrap();
            let _ = allocator.alloc((NUM_PAGES - 1) / 3).unwrap();

            allocator.free(p);

            allocator
                .alloc(NUM_PAGES / 2)
                .expect_none("requested memory should not have fit")
        };
    }

    fn assert_allocated(allocator: &mut BitmapAllocator, start: usize, count: usize) {
        for i in start..start + count - 1 {
            assert_eq!(
                unsafe { allocator.descriptors.add(i).read() },
                PageDescriptor {
                    flags: PageFlags::TAKEN
                }
            );
        }

        assert_eq!(
            unsafe { allocator.descriptors.add(start + count - 1).read() },
            PageDescriptor {
                flags: PageFlags::LAST
            }
        );
    }

    fn assert_free(allocator: &mut BitmapAllocator, start: usize, count: usize) {
        for i in start..start + count {
            assert_eq!(
                unsafe { allocator.descriptors.add(i).read() },
                PageDescriptor {
                    flags: PageFlags::empty()
                }
            );
        }
    }
}
