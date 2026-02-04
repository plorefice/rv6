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

use core::{mem::size_of, ptr, slice};

use bitflags::bitflags;

use crate::mm::{
    Align,
    addr::PhysAddr,
    allocator::{AllocatorError, Frame, FrameAllocator},
};

bitflags! {
    /// Allocation status of a page.
    #[derive(Debug, Clone, PartialEq, Eq)]
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
pub struct BitmapAllocator<const N: usize> {
    descriptors: &'static mut [PageDescriptor],
    base_addr: PhysAddr,
    num_pages: usize,
}

impl<const N: usize> BitmapAllocator<N> {
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

        let total_mem_size = (end - start).as_usize();
        let total_num_pages = total_mem_size / N;

        // A portion of memory starting from `start` will be reserved to hold page descriptors.
        // Memory available for allocation starts after this reserved memory.
        let reserved_mem = total_num_pages * size_of::<PageDescriptor>();
        let avail_mem_start = (start + reserved_mem).align_up(N);
        let avail_mem_size = end - avail_mem_start;
        let avail_pages = avail_mem_size.as_usize() / N;

        // SAFETY: `start` is aligned and must point to a valid memory region.
        let descriptors = unsafe {
            slice::from_raw_parts_mut(start.as_usize() as *mut PageDescriptor, avail_pages)
        };

        // Initially mark all pages as free
        for descr in descriptors.iter_mut() {
            *descr = PageDescriptor {
                flags: PageFlags::empty(),
            };
        }

        Ok(Self {
            descriptors,
            base_addr: avail_mem_start,
            num_pages: avail_pages,
        })
    }
}

impl<const N: usize> FrameAllocator<N> for BitmapAllocator<N> {
    fn alloc(&mut self, count: usize) -> Option<Frame> {
        let mut i: usize = 0;

        'outer: while i < self.num_pages {
            let descr = &mut self.descriptors[i];

            // Page already taken => keep going.
            if descr.flags.intersects(PageFlags::TAKEN | PageFlags::LAST) {
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
                let descr = &mut self.descriptors[j];

                if descr.flags.intersects(PageFlags::TAKEN | PageFlags::LAST) {
                    i = j;
                    continue 'outer;
                }
            }

            // If we get here, we managed to find `count` free pages.
            for j in i..i + count {
                self.descriptors[j].flags |= if j == i + count - 1 {
                    PageFlags::LAST
                } else {
                    PageFlags::TAKEN
                };
            }

            return Some(Frame {
                paddr: self.base_addr + i * N,
                ptr: ptr::null_mut(),
            });
        }

        None
    }

    fn free(&mut self, frame: Frame) {
        let offset = (frame.phys() - self.base_addr).as_usize() / N;

        for i in offset..self.num_pages {
            let flags = &mut self.descriptors[i].flags;

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
    use alloc::alloc::Layout;

    use lazy_static::lazy_static;

    use super::*;

    const PAGE_SIZE: usize = 4096;
    const NUM_PAGES: usize = 32;
    const MEM_SIZE: usize = NUM_PAGES * PAGE_SIZE;

    #[test]
    fn construction() {
        let (base, allocator) = create_allocator();

        assert_eq!(allocator.num_pages, NUM_PAGES - 1);
        assert_eq!(allocator.descriptors.as_ptr() as usize, base);
        assert_eq!(allocator.base_addr.data(), base + PAGE_SIZE);

        for i in 0..NUM_PAGES - 1 {
            assert_eq!(
                allocator.descriptors[i as usize],
                PageDescriptor {
                    flags: PageFlags::empty()
                }
            );
        }
    }

    #[test]
    fn invalid_addresses() {
        for t in &[
            (1, PAGE_SIZE),
            (PAGE_SIZE, 2 * PAGE_SIZE - 1),
            (1, PAGE_SIZE - 1),
        ] {
            unsafe {
                assert!(matches!(
                    BitmapAllocator::<_, PAGE_SIZE>::init(
                        PhysAddr::new_unchecked(t.0),
                        PhysAddr::new_unchecked(t.1)
                    ),
                    Err(AllocatorError::UnalignedAddress)
                ));
            }
        }
    }

    #[test]
    fn invalid_page_size() {
        assert!(matches!(
            unsafe {
                BitmapAllocator::<_, 0>::init(
                    PhysAddr::new_unchecked(0),
                    PhysAddr::new_unchecked(PAGE_SIZE),
                )
            },
            Err(AllocatorError::InvalidPageSize)
        ));

        assert!(matches!(
            unsafe {
                BitmapAllocator::<_, 3>::init(
                    PhysAddr::new_unchecked(0),
                    PhysAddr::new_unchecked(PAGE_SIZE),
                )
            },
            Err(AllocatorError::InvalidPageSize)
        ));

        assert!(matches!(
            unsafe {
                BitmapAllocator::<_, 24>::init(
                    PhysAddr::new_unchecked(0),
                    PhysAddr::new_unchecked(PAGE_SIZE),
                )
            },
            Err(AllocatorError::InvalidPageSize)
        ));

        assert!(matches!(
            unsafe {
                BitmapAllocator::<_, { PAGE_SIZE - 1 }>::init(
                    PhysAddr::new_unchecked(0),
                    PhysAddr::new_unchecked(PAGE_SIZE),
                )
            },
            Err(AllocatorError::InvalidPageSize)
        ));

        assert!(matches!(
            unsafe {
                BitmapAllocator::<_, { PAGE_SIZE + 1 }>::init(
                    PhysAddr::new_unchecked(0),
                    PhysAddr::new_unchecked(PAGE_SIZE),
                )
            },
            Err(AllocatorError::InvalidPageSize)
        ));
    }

    #[test]
    fn single_page() {
        let (_, mut allocator) = create_allocator();

        let ptr = allocator.alloc(1).expect("allocation failed");
        assert_allocated(&mut allocator, 0, 1);

        allocator.free(ptr);
        assert_free(&mut allocator, 0, 1);
    }

    #[test]
    fn multiple_pages() {
        let (_, mut allocator) = create_allocator();

        let ptr = allocator.alloc(4).expect("allocation failed");
        assert_allocated(&mut allocator, 0, 4);

        allocator.free(ptr);
        assert_free(&mut allocator, 0, 4);
    }

    #[test]
    fn multiple_allocations() {
        let (_, mut allocator) = create_allocator();

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
    }

    #[test]
    fn reuse_pages() {
        let (_, mut allocator) = create_allocator();

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
    }

    #[test]
    fn big_allocation() {
        let (_, mut allocator) = create_allocator();

        assert_eq!(allocator.alloc(NUM_PAGES as usize), None);
        assert_eq!(allocator.alloc(2 * NUM_PAGES as usize), None);

        allocator.alloc(1).expect("allocation failed");
    }

    #[test]
    fn spare_allocation() {
        let (_, mut allocator) = create_allocator();

        let _ = allocator.alloc((NUM_PAGES - 1) as usize / 3).unwrap();
        let p = allocator.alloc((NUM_PAGES - 1) as usize / 3).unwrap();
        let _ = allocator.alloc((NUM_PAGES - 1) as usize / 3).unwrap();

        allocator.free(p);

        assert_eq!(
            allocator.alloc(NUM_PAGES as usize / 2),
            None,
            "requested memory shou  ld not have fit"
        );
    }

    // --- Test types and utilities ---

    impl Align<u64> for u64 {
        fn align_up(&self, align: u64) -> Self {
            (self + align - 1) & !(align - 1)
        }

        fn align_down(&self, align: u64) -> Self {
            self & !(align - 1)
        }

        fn is_aligned(&self, align: u64) -> bool {
            self & (align - 1) == 0
        }
    }

    lazy_static! {
        // Page-aligned chunk of memory
        static ref CHUNK: usize = unsafe {
            alloc::alloc::alloc(
                Layout::from_size_align(MEM_SIZE as usize, PAGE_SIZE as usize).unwrap(),
            )
        } as usize;
    }

    /// Creates a new allocator and returns both the base address and the allocator itself.
    fn create_allocator() -> (usize, BitmapAllocator<PAGE_SIZE>) {
        unsafe {
            (
                *CHUNK,
                BitmapAllocator::init(
                    PhysAddr::new_unchecked(*CHUNK),
                    PhysAddr::new_unchecked(*CHUNK + MEM_SIZE),
                )
                .unwrap(),
            )
        }
    }

    fn assert_allocated<const N: usize>(
        allocator: &mut BitmapAllocator<N>,
        start: usize,
        count: usize,
    ) {
        for i in start..start + count - 1 {
            assert_eq!(
                allocator.descriptors[i as usize],
                PageDescriptor {
                    flags: PageFlags::TAKEN
                }
            );
        }

        assert_eq!(
            allocator.descriptors[(start + count - 1) as usize],
            PageDescriptor {
                flags: PageFlags::LAST
            }
        );
    }

    fn assert_free<const N: usize>(allocator: &mut BitmapAllocator<N>, start: usize, count: usize) {
        for i in start..start + count {
            assert_eq!(
                allocator.descriptors[i as usize],
                PageDescriptor {
                    flags: PageFlags::empty()
                }
            );
        }
    }
}
