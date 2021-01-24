//! A simple bitmap allocator for physical pages.
//!
//! The allocator keeps track of free and allocated pages by keeping a list of page descriptors
//! at the top of the managed memory, hence the term "bitmap".
//!
//! Each page can either be marked as free, as a part of a larger chunk of allocated memory or
//! as the last (or only) page in a chunk. Discriminating between these last two states allows to
//! free a memory chunk by knowing only its pointer and not its size.
//!
//! # Complexity
//!
//! Freeing a page in a bitmap allocator has `O(1)` complexity, but allocation is more expensive
//! (`O(n)`) since we need to find a large-enough chunk of free pages.

use core::mem::size_of;

use bitflags::bitflags;
use riscv::{addr::Align, PhysAddr};

use super::{AllocatorError, FrameAllocator};

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
pub struct BitmapAllocator<const N: u64> {
    descriptors: *mut PageDescriptor,
    base_addr: PhysAddr,
    num_pages: u64,
}

impl<const N: u64> BitmapAllocator<N> {
    /// Creates a new bitmap allocator taking ownership of the memory delimited by addresses
    /// `start` and `end`, and allocating pages of size `page_size`.
    ///
    /// Returns an `AllocationError` if any of the following conditions are not met:
    ///  - `start` and `end` are page-aligned,
    ///  - `page_size` is a non-zero power of two.
    ///
    /// # Safety
    ///
    /// There can be no guarantee that the memory being passed to the allocator isn't already in
    /// use by the system, so tread carefully here.
    pub unsafe fn init(start: PhysAddr, end: PhysAddr) -> Result<Self, AllocatorError> {
        if N == 0 || !N.is_power_of_two() {
            return Err(AllocatorError::InvalidPageSize);
        }
        if !start.is_aligned(N) || !end.is_aligned(N) {
            return Err(AllocatorError::UnalignedAddress);
        }

        let total_mem_size = u64::from(end - start);
        let total_num_pages = total_mem_size / N;

        // A portion of memory starting from `start` will be reserved to hold page descriptors.
        // Memory available for allocation starts after this reserved memory.
        let reserved_mem = total_num_pages * size_of::<PageDescriptor>() as u64;
        let avail_mem_start = (start + reserved_mem).align_up(N);
        let avail_mem_size = end - avail_mem_start;
        let avail_pages = u64::from(avail_mem_size) / N;

        // Initially mark all pages as free
        let descriptors = u64::from(start) as *mut PageDescriptor;
        for i in 0..avail_pages {
            descriptors.add(i as usize).write(PageDescriptor {
                flags: PageFlags::empty(),
            });
        }

        Ok(Self {
            descriptors,
            base_addr: avail_mem_start,
            num_pages: avail_pages,
        })
    }
}

impl<const N: u64> FrameAllocator<N> for BitmapAllocator<N> {
    unsafe fn alloc(&mut self, count: usize) -> Option<PhysAddr> {
        let mut i = 0;

        'outer: while i < self.num_pages {
            let descr = self.descriptors.add(i as usize);

            // Page already taken => keep going.
            if (*descr)
                .flags
                .intersects(PageFlags::TAKEN | PageFlags::LAST)
            {
                i += 1;
                continue;
            }

            // Not enough pages left => abort.
            if self.num_pages - i < count as u64 {
                return None;
            }

            // Check if enough contiguous pages are free
            // NOTE: `x` here to make clippy happy.
            let x = i;
            for j in x..x + count as u64 {
                let descr = self.descriptors.add(j as usize);

                if (*descr)
                    .flags
                    .intersects(PageFlags::TAKEN | PageFlags::LAST)
                {
                    i = j;
                    continue 'outer;
                }
            }

            // If we get here, we managed to find `count` free pages.
            for j in i..i + count as u64 {
                (*self.descriptors.add(j as usize)).flags |= if j == i + count as u64 - 1 {
                    PageFlags::LAST
                } else {
                    PageFlags::TAKEN
                };
            }

            return Some(self.base_addr + i * N);
        }

        None
    }

    unsafe fn free(&mut self, address: PhysAddr) {
        let offset = u64::from(address - self.base_addr) / N;

        for i in offset..self.num_pages {
            let descr = self.descriptors.add(i as usize);
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

    use crate::mm::page::PAGE_SIZE;

    const NUM_PAGES: usize = 32;
    const MEM_SIZE: usize = NUM_PAGES * PAGE_SIZE;
    const MEM_BASE: usize = 0x8700_0000;

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn construction() {
        let allocator = unsafe {
            BitmapAllocator::init(
                PhysicalAddress::new(MEM_BASE),
                PhysicalAddress::new(MEM_BASE) + MEM_SIZE,
                PAGE_SIZE,
            )
            .unwrap()
        };

        assert_eq!(allocator.page_size, PAGE_SIZE);
        assert_eq!(allocator.num_pages, NUM_PAGES - 1);
        assert_eq!(allocator.descriptors as *const (), MEM_BASE as *const ());
        assert_eq!(
            allocator.base_addr,
            PhysicalAddress::new(MEM_BASE) + PAGE_SIZE,
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

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn invalid_addresses() {
        for t in &[
            (1, PAGE_SIZE),
            (PAGE_SIZE, 2 * PAGE_SIZE - 1),
            (1, PAGE_SIZE - 1),
        ] {
            unsafe {
                assert!(matches!(
                    BitmapAllocator::init(
                        PhysicalAddress::new(t.0),
                        PhysicalAddress::new(t.1),
                        PAGE_SIZE,
                    ),
                    Err(AllocatorError::UnalignedAddress)
                ));
            }
        }
    }

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn invalid_page_size() {
        for t in &[0, 3, 24, PAGE_SIZE - 1, PAGE_SIZE + 2] {
            unsafe {
                assert!(matches!(
                    BitmapAllocator::init(
                        PhysicalAddress::new(0),
                        PhysicalAddress::new(PAGE_SIZE),
                        *t,
                    ),
                    Err(AllocatorError::InvalidPageSize)
                ));
            }
        }
    }

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn single_page() {
        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(MEM_BASE),
                PhysicalAddress::new(MEM_BASE) + MEM_SIZE,
                PAGE_SIZE,
            )
            .unwrap();

            let ptr = allocator.alloc(1).expect("allocation failed");
            assert_allocated(&mut allocator, 0, 1);

            allocator.free(ptr);
            assert_free(&mut allocator, 0, 1);
        };
    }

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn multiple_pages() {
        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(MEM_BASE),
                PhysicalAddress::new(MEM_BASE) + MEM_SIZE,
                PAGE_SIZE,
            )
            .unwrap();

            let ptr = allocator.alloc(4).expect("allocation failed");
            assert_allocated(&mut allocator, 0, 4);

            allocator.free(ptr);
            assert_free(&mut allocator, 0, 4);
        };
    }

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn multiple_allocations() {
        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(MEM_BASE),
                PhysicalAddress::new(MEM_BASE) + MEM_SIZE,
                PAGE_SIZE,
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

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn reuse_pages() {
        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(MEM_BASE),
                PhysicalAddress::new(MEM_BASE) + MEM_SIZE,
                PAGE_SIZE,
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

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn big_allocation() {
        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(MEM_BASE),
                PhysicalAddress::new(MEM_BASE) + MEM_SIZE,
                PAGE_SIZE,
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

    #[cfg_attr(not(target_arch = "riscv64"), test_case)]
    fn spare_allocation() {
        unsafe {
            let mut allocator = BitmapAllocator::init(
                PhysicalAddress::new(MEM_BASE),
                PhysicalAddress::new(MEM_BASE) + MEM_SIZE,
                PAGE_SIZE,
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
