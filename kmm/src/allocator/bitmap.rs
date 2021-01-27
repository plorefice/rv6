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

use crate::PhysicalAddress;

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
pub struct BitmapAllocator<A, const N: u64> {
    descriptors: *mut PageDescriptor,
    base_addr: A,
    num_pages: u64,
}

impl<A, const N: u64> BitmapAllocator<A, N>
where
    A: PhysicalAddress<u64>,
{
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
    pub unsafe fn init(start: A, end: A) -> Result<Self, AllocatorError> {
        if N == 0 || !N.is_power_of_two() {
            return Err(AllocatorError::InvalidPageSize);
        }
        if !start.is_aligned(N) || !end.is_aligned(N) {
            return Err(AllocatorError::UnalignedAddress);
        }

        let total_mem_size: u64 = (end - start).into();
        let total_num_pages = total_mem_size / N;

        // A portion of memory starting from `start` will be reserved to hold page descriptors.
        // Memory available for allocation starts after this reserved memory.
        let reserved_mem = total_num_pages * size_of::<PageDescriptor>() as u64;
        let avail_mem_start = (start + reserved_mem).align_up(N);
        let avail_mem_size = end - avail_mem_start;
        let avail_pages: u64 = avail_mem_size.into() / N;

        // Initially mark all pages as free
        let descriptors = <A as Into<u64>>::into(start) as *mut PageDescriptor;
        for i in 0..avail_pages {
            unsafe {
                descriptors.add(i as usize).write(PageDescriptor {
                    flags: PageFlags::empty(),
                })
            };
        }

        Ok(Self {
            descriptors,
            base_addr: avail_mem_start,
            num_pages: avail_pages,
        })
    }
}

impl<A, const N: u64> FrameAllocator<A, N> for BitmapAllocator<A, N>
where
    A: PhysicalAddress<u64>,
{
    unsafe fn alloc(&mut self, count: usize) -> Option<A> {
        let mut i = 0;

        'outer: while i < self.num_pages {
            let descr = unsafe { self.descriptors.add(i as usize).as_ref() }.unwrap();

            // Page already taken => keep going.
            if descr.flags.intersects(PageFlags::TAKEN | PageFlags::LAST) {
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
                let descr = unsafe { self.descriptors.add(i as usize).as_ref() }.unwrap();

                if descr.flags.intersects(PageFlags::TAKEN | PageFlags::LAST) {
                    i = j;
                    continue 'outer;
                }
            }

            // If we get here, we managed to find `count` free pages.
            for j in i..i + count as u64 {
                let descr = unsafe { self.descriptors.add(j as usize).as_mut() }.unwrap();

                descr.flags |= if j == i + count as u64 - 1 {
                    PageFlags::LAST
                } else {
                    PageFlags::TAKEN
                };
            }

            return Some(self.base_addr + i * N);
        }

        None
    }

    unsafe fn free(&mut self, address: A) {
        let offset: u64 = (address - self.base_addr).into() / N;

        for i in offset..self.num_pages {
            let descr = unsafe { self.descriptors.add(i as usize).as_mut() }.unwrap();
            let flags = &mut descr.flags;

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
    use std::alloc::Layout;

    use lazy_static::lazy_static;

    use crate::{AddressOps, Align};

    use super::*;

    const PAGE_SIZE: u64 = 4096;
    const NUM_PAGES: u64 = 32;
    const MEM_SIZE: u64 = NUM_PAGES * PAGE_SIZE;

    #[test]
    fn construction() {
        let (base, allocator) = create_allocator();

        assert_eq!(allocator.num_pages, NUM_PAGES - 1);
        assert_eq!(allocator.descriptors as u64, base);
        assert_eq!(allocator.base_addr, base + PAGE_SIZE,);

        for i in 0..NUM_PAGES - 1 {
            assert_eq!(
                unsafe { allocator.descriptors.add(i as usize).read() },
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
                    BitmapAllocator::<_, PAGE_SIZE>::init(t.0, t.1),
                    Err(AllocatorError::UnalignedAddress)
                ));
            }
        }
    }

    #[test]
    fn invalid_page_size() {
        assert!(matches!(
            unsafe { BitmapAllocator::<_, 0>::init(0, PAGE_SIZE) },
            Err(AllocatorError::InvalidPageSize)
        ));

        assert!(matches!(
            unsafe { BitmapAllocator::<_, 3>::init(0, PAGE_SIZE) },
            Err(AllocatorError::InvalidPageSize)
        ));

        assert!(matches!(
            unsafe { BitmapAllocator::<_, 24>::init(0, PAGE_SIZE) },
            Err(AllocatorError::InvalidPageSize)
        ));

        assert!(matches!(
            unsafe { BitmapAllocator::<_, { PAGE_SIZE - 1 }>::init(0, PAGE_SIZE) },
            Err(AllocatorError::InvalidPageSize)
        ));

        assert!(matches!(
            unsafe { BitmapAllocator::<_, { PAGE_SIZE + 2 }>::init(0, PAGE_SIZE) },
            Err(AllocatorError::InvalidPageSize)
        ));
    }

    #[test]
    fn single_page() {
        unsafe {
            let (_, mut allocator) = create_allocator();

            let ptr = allocator.alloc(1).expect("allocation failed");
            assert_allocated(&mut allocator, 0, 1);

            allocator.free(ptr);
            assert_free(&mut allocator, 0, 1);
        }
    }

    #[test]
    fn multiple_pages() {
        unsafe {
            let (_, mut allocator) = create_allocator();

            let ptr = allocator.alloc(4).expect("allocation failed");
            assert_allocated(&mut allocator, 0, 4);

            allocator.free(ptr);
            assert_free(&mut allocator, 0, 4);
        };
    }

    #[test]
    fn multiple_allocations() {
        unsafe {
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
        };
    }

    #[test]
    fn reuse_pages() {
        unsafe {
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
        };
    }

    #[test]
    fn big_allocation() {
        unsafe {
            let (_, mut allocator) = create_allocator();

            assert_eq!(allocator.alloc(NUM_PAGES as usize), None);
            assert_eq!(allocator.alloc(2 * NUM_PAGES as usize), None);

            allocator.alloc(1).expect("allocation failed");
        };
    }

    #[test]
    fn spare_allocation() {
        unsafe {
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
        };
    }

    // --- Test types and utilities ---

    impl PhysicalAddress<u64> for u64 {}

    impl AddressOps<u64> for u64 {}

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
        static ref CHUNK: u64 = unsafe {
            std::alloc::alloc(
                Layout::from_size_align(MEM_SIZE as usize, PAGE_SIZE as usize).unwrap(),
            )
        } as u64;
    }

    /// Creates a new allocator and returns both the base address and the allocator itself.
    fn create_allocator() -> (u64, BitmapAllocator<u64, PAGE_SIZE>) {
        unsafe {
            (
                *CHUNK,
                BitmapAllocator::init(*CHUNK, *CHUNK + MEM_SIZE).unwrap(),
            )
        }
    }

    fn assert_allocated<const N: u64>(
        allocator: &mut BitmapAllocator<u64, N>,
        start: u64,
        count: u64,
    ) {
        for i in start..start + count - 1 {
            assert_eq!(
                unsafe { allocator.descriptors.add(i as usize).read() },
                PageDescriptor {
                    flags: PageFlags::TAKEN
                }
            );
        }

        assert_eq!(
            unsafe {
                allocator
                    .descriptors
                    .add((start + count - 1) as usize)
                    .read()
            },
            PageDescriptor {
                flags: PageFlags::LAST
            }
        );
    }

    fn assert_free<const N: u64>(allocator: &mut BitmapAllocator<u64, N>, start: u64, count: u64) {
        for i in start..start + count {
            assert_eq!(
                unsafe { allocator.descriptors.add(i as usize).read() },
                PageDescriptor {
                    flags: PageFlags::empty()
                }
            );
        }
    }
}
