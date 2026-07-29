//! A simple bitmap allocator for physical pages.
//!
//! The allocator keeps track of free and allocated pages by keeping a list of page descriptors
//! at the top of the managed memory, hence the term "bitmap".
//!
//! Each page can either be marked as free, as a part of a larger chunk of allocated memory or
//! as the last (or only) page in a chunk. Discriminating between these last two states allows to
//! free a memory chunk by knowing only its base address and not its size.
//!
//! # Freeing
//!
//! [`FrameAllocator::free`] must be called with the **base** address returned by
//! [`FrameAllocator::alloc`]. Freeing a page from the middle of a multi-page allocation,
//! freeing an address outside the managed range, or double-freeing panics.
//!
//! # Complexity
//!
//! Freeing a chunk is `O(k)` in the number of pages in that chunk. Allocation is `O(n)` in the
//! number of managed pages, since we need to find a large-enough run of free pages.

use core::{mem::size_of, slice};

use bitflags::bitflags;

use crate::{
    arch::hal,
    mm::{
        addr::{Align, PhysAddr},
        allocator::{AllocatorError, Frame, FrameAllocator},
    },
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
            let ptr: *mut PageDescriptor = hal::mm::phys_to_virt(start).as_mut_ptr();
            slice::from_raw_parts_mut(ptr, avail_pages)
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
        if count == 0 {
            return None;
        }

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

            let paddr = self.base_addr + i * N;
            return Some(Frame {
                // SAFETY: `paddr` is guaranteed to be a valid physical address.
                ptr: unsafe { hal::mm::phys_to_virt(paddr).as_mut_ptr() },
                paddr,
            });
        }

        None
    }

    fn free(&mut self, frame: Frame) {
        let paddr = frame.phys();

        assert!(
            paddr.is_aligned(N),
            "Trying to free an unaligned address: {paddr:#x}"
        );

        if paddr < self.base_addr {
            panic!("Trying to free a page outside the managed range: {paddr:#x}");
        }

        let offset = (paddr - self.base_addr).as_usize() / N;
        if offset >= self.num_pages {
            panic!("Trying to free a page outside the managed range: {paddr:#x}");
        }

        if self.descriptors[offset].flags.is_empty() {
            panic!("Trying to free an unallocated page!");
        }

        // Non-terminal pages of a chunk are marked TAKEN only. If the previous page is TAKEN,
        // this address sits in the middle (or at the end) of a larger allocation.
        if offset > 0
            && self.descriptors[offset - 1]
                .flags
                .contains(PageFlags::TAKEN)
        {
            panic!("Trying to free from the middle of an allocation!");
        }

        for i in offset..self.num_pages {
            let flags = &mut self.descriptors[i].flags;
            let is_last = flags.contains(PageFlags::LAST);

            if flags.is_empty() {
                panic!("Corrupted allocator state: hole in allocation at page {i}");
            }

            *flags = PageFlags::empty();

            if is_last {
                return;
            }
        }

        panic!("Corrupted allocator state: allocation missing LAST marker");
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

    #[test]
    fn zero_count_allocation() {
        let (_, mut allocator) = create_allocator();
        assert!(allocator.alloc(0).is_none());
    }

    #[test]
    #[should_panic(expected = "Trying to free from the middle of an allocation!")]
    fn free_mid_chunk() {
        let (_, mut allocator) = create_allocator();

        let base = allocator.alloc(4).expect("allocation failed");
        // SAFETY: freeing only for the panic check; address is within the managed range.
        let mid = unsafe { Frame::unmapped(base.phys() + PAGE_SIZE) };
        allocator.free(mid);
    }

    #[test]
    #[should_panic(expected = "Trying to free from the middle of an allocation!")]
    fn free_last_page_of_chunk() {
        let (_, mut allocator) = create_allocator();

        let base = allocator.alloc(4).expect("allocation failed");
        // SAFETY: freeing only for the panic check; address is within the managed range.
        let last = unsafe { Frame::unmapped(base.phys() + 3 * PAGE_SIZE) };
        allocator.free(last);
    }

    #[test]
    #[should_panic(expected = "Trying to free an unallocated page!")]
    fn double_free() {
        let (_, mut allocator) = create_allocator();

        let frame = allocator.alloc(1).expect("allocation failed");
        // SAFETY: reconstruct a frame with the same physical address after the first free.
        let paddr = frame.phys();
        allocator.free(frame);
        allocator.free(unsafe { Frame::unmapped(paddr) });
    }

    #[test]
    #[should_panic(expected = "Trying to free a page outside the managed range")]
    fn free_below_managed_range() {
        let (base, mut allocator) = create_allocator();

        // Descriptor region / below avail_mem_start
        // SAFETY: intentionally out of range to exercise the check.
        allocator.free(unsafe { Frame::unmapped(PhysAddr::new_unchecked(base)) });
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
