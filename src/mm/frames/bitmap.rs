use super::PhysicalAddress;

/// A frame allocator storing page state as a bitmap of possibly-contiguous pages.
///
/// Freeing a page in a bitmap allocator has `O(1)` complexity, but allocation is more expensive
/// (`O(n)`) since we need to find a large-enough chunk of free pages.
pub struct BitmapAllocator;

impl BitmapAllocator {
    pub unsafe fn init(_start: PhysicalAddress, _end: PhysicalAddress, _page_size: usize) -> Self {
        todo!()
    }
}
