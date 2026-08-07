//! Interfaces for DMA-capable memory addresses and allocation.

use core::{
    alloc::Layout,
    mem::{self, ManuallyDrop, MaybeUninit},
    ptr::{self, NonNull},
};

use crate::mm::addr::DmaAddr;

/// Error type for DMA allocation failures.
#[derive(Debug, Clone)]
pub enum DmaAllocError {
    /// Not enough memory available to satisfy the allocation request.
    OutOfMemory,
}

/// Direction of a DMA transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaDirection {
    /// Device to memory transfer.
    ToDevice,
    /// Memory to device transfer.
    FromDevice,
    /// Bidirectional transfer.
    Bidirectional,
}

/// Non-owning description of a DMA buffer.
///
/// Used as the argument to [`DmaAllocator::free_raw`] so that freeing does not
/// interact with [`DmaBuf`]'s [`Drop`] implementation.
#[derive(Debug, Clone, Copy)]
pub struct DmaBufParts {
    /// CPU-accessible pointer to the buffer.
    pub ptr: NonNull<u8>,
    /// Device-visible DMA address of the buffer.
    pub dma_addr: DmaAddr,
    /// Length of the buffer in bytes.
    pub size: usize,
    /// Alignment of the buffer.
    pub align: usize,
}

/// A trait for DMA-capable memory allocators.
///
/// Provides methods for allocating and freeing memory regions that can be used
/// for DMA operations, as well as synchronizing memory between the CPU and devices.
///
/// Owning DMA types ([`DmaBuf`], [`DmaObject`], [`DmaSlice`]) are valid only while the borrowed
/// allocator is, and [`Drop`] frees through that borrow.
pub trait DmaAllocator: Send + Sync {
    /// Allocates a DMA-capable memory region with the specified layout.
    ///
    /// The returned [`DmaBuf`] frees the region when dropped.
    fn alloc_raw<'a>(&'a self, layout: Layout) -> Result<DmaBuf<'a>, DmaAllocError>;

    /// Allocates a DMA-capable memory region with the specified layout, initialized to zero.
    fn alloc_raw_zeroed<'a>(&'a self, layout: Layout) -> Result<DmaBuf<'a>, DmaAllocError> {
        let buf = self.alloc_raw(layout)?;

        // SAFETY: buf.ptr is known to be a valid pointer
        unsafe {
            ptr::write_bytes(buf.ptr.as_ptr(), 0, buf.size);
        }

        Ok(buf)
    }

    /// Frees a previously allocated DMA-capable memory region.
    ///
    /// # Safety
    ///
    /// The provided parts must describe a buffer previously allocated by this
    /// allocator that has not already been freed.
    unsafe fn free_raw(&self, parts: DmaBufParts);

    /// Synchronizes the memory region for device access.
    ///
    /// The implementation should ensure that any CPU-side writes are visible to the device
    /// after this call.
    fn sync_for_device(&self, addr: DmaAddr, len: usize, direction: DmaDirection);

    /// Synchronizes the memory region for CPU access.
    ///
    /// The implementation should ensure that any device-side writes are visible to the CPU
    /// after this call.
    fn sync_for_cpu(&self, addr: DmaAddr, len: usize, direction: DmaDirection);
}

/// Extension methods for DMA allocators.
///
/// Provides higher-level allocation methods for typed objects. Allocated
/// [`DmaObject`]s and [`DmaSlice`]s free their backing memory on [`Drop`].
pub trait DmaAllocatorExt: DmaAllocator {
    /// Allocates a DMA-capable memory region for an object of type `T`.
    ///
    /// The provided value is copied into the allocated region. If you don't need to initialize
    /// the memory, consider using [`alloc_uninit`] or [`alloc_zeroed`] instead
    fn alloc<'a, T: DmaSafe>(&'a self, val: T) -> Result<DmaObject<'a, T>, DmaAllocError> {
        let mut obj = self.alloc_uninit::<T>()?;
        // SAFETY: obj points to at least size_of::<T>() bytes, properly mapped for CPU access.
        unsafe {
            ptr::write(obj.as_mut_ptr().cast::<T>(), val);
        }
        // SAFETY: by construction we just initialized all bytes of T.
        Ok(unsafe { obj.assume_init() })
    }

    /// Allocates a DMA-capable memory region for an uninitialized object of type `T`.
    fn alloc_uninit<'a, T: DmaSafe>(
        &'a self,
    ) -> Result<DmaObject<'a, MaybeUninit<T>>, DmaAllocError> {
        let layout = Layout::new::<T>();

        let buf = self.alloc_raw(layout)?;
        let (parts, alloc) = buf.into_parts();

        // SAFETY: `parts.ptr` is known to be a valid pointer
        let ptr = unsafe { NonNull::new_unchecked(parts.ptr.as_ptr() as *mut MaybeUninit<T>) };

        // SAFETY: by construction; ownership transferred from DmaBuf via into_parts
        Ok(unsafe { DmaObject::new_unchecked(ptr, parts.dma_addr, parts.size, alloc) })
    }

    /// Allocates a DMA-capable memory region for an object of type `T`, initializing it to zero.
    fn alloc_zeroed<'a, T: DmaSafe>(&'a self) -> Result<DmaObject<'a, T>, DmaAllocError> {
        let mut obj = self.alloc_uninit::<T>()?;
        // SAFETY: obj points to at least size_of::<T>() bytes, properly mapped for CPU access.
        unsafe {
            ptr::write_bytes(obj.as_mut_ptr().cast::<u8>(), 0, mem::size_of::<T>());
        }
        // SAFETY: by construction we just initialized all bytes of T to zero.
        Ok(unsafe { obj.assume_init() })
    }

    /// Allocates a DMA-capable memory region for a slice of `T` with the specified length.
    fn alloc_slice_from<'a, T: DmaSafe>(
        &'a self,
        src: &[T],
    ) -> Result<DmaSlice<'a, T>, DmaAllocError> {
        let mut slice = self.alloc_slice_uninit::<T>(src.len())?;
        // SAFETY: slice points to at least len * size_of::<T>() bytes, properly mapped for CPU access.
        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), slice.as_mut_ptr() as _, src.len());
        }
        // SAFETY: by construction we just initialized all bytes of T.
        Ok(unsafe { slice.assume_init() })
    }

    /// Allocates a DMA-capable memory region for an uninitialized slice of `T` with the specified length.
    fn alloc_slice_uninit<'a, T: DmaSafe>(
        &'a self,
        len: usize,
    ) -> Result<DmaSlice<'a, MaybeUninit<T>>, DmaAllocError> {
        let layout = Layout::array::<T>(len).map_err(|_| DmaAllocError::OutOfMemory)?;

        let buf = self.alloc_raw(layout)?;
        let (parts, alloc) = buf.into_parts();

        // SAFETY: `parts.ptr` is known to be a valid pointer
        let ptr = unsafe { NonNull::new_unchecked(parts.ptr.as_ptr() as *mut MaybeUninit<T>) };

        // SAFETY: by construction; ownership transferred from DmaBuf via into_parts
        Ok(unsafe { DmaSlice::new_unchecked(ptr, parts.dma_addr, len, parts.size, alloc) })
    }

    /// Allocates a DMA-capable memory region for a slice of `T` with the specified length, initializing it to zero.
    fn alloc_slice_zeroed<'a, T: DmaSafe>(
        &'a self,
        len: usize,
    ) -> Result<DmaSlice<'a, T>, DmaAllocError> {
        let mut slice = self.alloc_slice_uninit::<T>(len)?;
        // SAFETY: slice points to at least len * size_of::<T>() bytes, properly mapped for CPU access.
        unsafe {
            ptr::write_bytes(slice.as_mut_ptr().cast::<u8>(), 0, slice.size());
        }
        // SAFETY: by construction we just initialized all bytes of T to zero.
        Ok(unsafe { slice.assume_init() })
    }
}

/// Blanket implementation of `DmaAllocatorExt` for all `DmaAllocator` types.
impl<T: DmaAllocator + ?Sized> DmaAllocatorExt for T {}

/// Only allow types that are safe to DMA as raw bytes.
///
/// # Safety
///
/// The type must not contain any pointers or references that could lead to
/// undefined behavior when accessed by a DMA-capable device.
pub unsafe trait DmaSafe: Copy {}

/// Blanket implementation for all `Copy` types.
// SAFETY: all `Copy` types are safe to DMA as raw bytes
unsafe impl<T: Copy> DmaSafe for T {}

/// A DMA-capable buffer.
///
/// Frees its backing memory via the stored allocator when dropped.
pub struct DmaBuf<'a> {
    ptr: NonNull<u8>,
    dma_addr: DmaAddr,
    size: usize,
    align: usize,
    alloc: &'a dyn DmaAllocator,
}

// SAFETY: the memory `DmaBuf` points to is a permanent identity-mapped DMA allocation
unsafe impl<'a> Send for DmaBuf<'a> {}
// SAFETY: the memory `DmaBuf` points to is a permanent identity-mapped DMA allocation
unsafe impl<'a> Sync for DmaBuf<'a> {}

impl<'a> DmaBuf<'a> {
    /// Creates a new `DmaBuf` from the given components.
    ///
    /// The buffer will be freed via `alloc` when dropped.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid memory region of at least `size` bytes, and `dma_addr` must
    /// correspond to the physical address of that memory region. The region must have been
    /// allocated by `alloc`.
    pub unsafe fn new_unchecked(
        ptr: NonNull<u8>,
        dma_addr: DmaAddr,
        size: usize,
        align: usize,
        alloc: &'a dyn DmaAllocator,
    ) -> DmaBuf<'a> {
        DmaBuf {
            ptr,
            dma_addr,
            size,
            align,
            alloc,
        }
    }

    /// Rebuilds an owning buffer from raw parts.
    ///
    /// # Safety
    ///
    /// Same requirements as [`Self::new_unchecked`]. The caller transfers ownership of the
    /// memory described by `parts` to the returned buffer.
    pub unsafe fn from_parts(parts: DmaBufParts, alloc: &'a dyn DmaAllocator) -> DmaBuf<'a> {
        // SAFETY: caller upholds the same contract as new_unchecked
        unsafe { Self::new_unchecked(parts.ptr, parts.dma_addr, parts.size, parts.align, alloc) }
    }

    /// Disarms [`Drop`] and returns the buffer parts plus the allocator handle.
    ///
    /// The caller becomes responsible for freeing the memory (e.g. by rebuilding a
    /// [`DmaBuf`] / [`DmaObject`] / [`DmaSlice`], or by calling [`DmaAllocator::free_raw`]).
    pub fn into_parts(self) -> (DmaBufParts, &'a dyn DmaAllocator) {
        let this = ManuallyDrop::new(self);
        (
            DmaBufParts {
                ptr: this.ptr,
                dma_addr: this.dma_addr,
                size: this.size,
                align: this.align,
            },
            this.alloc,
        )
    }

    /// Disarms [`Drop`] and returns the buffer parts.
    ///
    /// The caller becomes responsible for the memory; the allocator handle is discarded.
    pub fn into_raw(self) -> DmaBufParts {
        self.into_parts().0
    }

    /// Returns a raw pointer to the buffer.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Returns a mutable raw pointer to the buffer.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Returns the DMA-capable physical address of the buffer.
    pub fn dma_addr(&self) -> DmaAddr {
        self.dma_addr
    }

    /// Returns the length of the buffer in bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the alignment of the buffer.
    pub fn align(&self) -> usize {
        self.align
    }

    /// Synchronizes the buffer for CPU access.
    ///
    /// See [`sync_for_cpu`](DmaAllocator::sync_for_cpu) for details.
    pub fn sync_for_cpu(&self, direction: DmaDirection) {
        self.alloc.sync_for_cpu(self.dma_addr, self.size, direction);
    }

    /// Synchronizes the buffer for device access.
    ///
    /// See [`sync_for_device`](DmaAllocator::sync_for_device) for details.
    pub fn sync_for_device(&self, direction: DmaDirection) {
        self.alloc
            .sync_for_device(self.dma_addr, self.size, direction);
    }
}

impl<'a> Drop for DmaBuf<'a> {
    fn drop(&mut self) {
        let parts = DmaBufParts {
            ptr: self.ptr,
            dma_addr: self.dma_addr,
            size: self.size,
            align: self.align,
        };
        // SAFETY: by construction, this buffer was allocated by self.alloc
        unsafe {
            self.alloc.free_raw(parts);
        }
    }
}

/// An object allocated in DMA-capable memory.
///
/// Frees its backing memory via the stored allocator when dropped.
pub struct DmaObject<'a, T> {
    ptr: NonNull<T>,   // CPU accessible pointer
    dma_addr: DmaAddr, // Device-visible physical address
    size: usize,       // Length in bytes
    alloc: &'a dyn DmaAllocator,
}

// SAFETY: the memory `DmaObject` points to is a permanent identity-mapped DMA allocation
unsafe impl<'a, T: Send> Send for DmaObject<'a, T> {}
// SAFETY: the memory `DmaObject` points to is a permanent identity-mapped DMA allocation
unsafe impl<'a, T: Sync> Sync for DmaObject<'a, T> {}

impl<'a, T> DmaObject<'a, T> {
    /// Creates a new `DmaObject` from the given components.
    ///
    /// The object will be freed via `alloc` when dropped.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid memory region of at least `size` bytes, and `dma_addr` must
    /// correspond to the physical address of that memory region. The region must have been
    /// allocated by `alloc`.
    pub unsafe fn new_unchecked(
        ptr: NonNull<T>,
        dma_addr: DmaAddr,
        size: usize,
        alloc: &'a dyn DmaAllocator,
    ) -> DmaObject<'a, T> {
        DmaObject {
            ptr,
            dma_addr,
            size,
            alloc,
        }
    }

    /// Returns the DMA-capable physical address of the object.
    pub fn dma_addr(&self) -> DmaAddr {
        self.dma_addr
    }

    /// Returns the length of the allocated object in bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns a raw pointer to the object.
    pub fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Returns a mutable raw pointer to the object.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Synchronizes the object for CPU access.
    ///
    /// See [`sync_for_cpu`](DmaAllocator::sync_for_cpu) for details.
    pub fn sync_for_cpu(&self, direction: DmaDirection) {
        self.alloc.sync_for_cpu(self.dma_addr, self.size, direction);
    }

    /// Synchronizes the object for device access.
    ///
    /// See [`sync_for_device`](DmaAllocator::sync_for_device) for details.
    pub fn sync_for_device(&self, direction: DmaDirection) {
        self.alloc
            .sync_for_device(self.dma_addr, self.size, direction);
    }
}

impl<'a, T> Drop for DmaObject<'a, T> {
    fn drop(&mut self) {
        let parts = DmaBufParts {
            ptr: self.ptr.cast::<u8>(),
            dma_addr: self.dma_addr,
            size: self.size,
            align: mem::align_of::<T>(),
        };
        // SAFETY: by construction, this object was allocated by self.alloc
        unsafe {
            self.alloc.free_raw(parts);
        }
    }
}

impl<'a, T> AsRef<T> for DmaObject<'a, T> {
    fn as_ref(&self) -> &T {
        // SAFETY: by construction
        unsafe { self.ptr.as_ref() }
    }
}

impl<'a, T> AsMut<T> for DmaObject<'a, T> {
    fn as_mut(&mut self) -> &mut T {
        // SAFETY: by construction
        unsafe { self.ptr.as_mut() }
    }
}

impl<'a, T: DmaSafe> DmaObject<'a, MaybeUninit<T>> {
    /// Assumes the object has been initialized and returns a `DmaObject<T>`.
    ///
    /// Ownership of the backing memory is transferred; [`Drop`] is not run on `self`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the memory has been properly initialized.
    pub unsafe fn assume_init(self) -> DmaObject<'a, T> {
        let this = ManuallyDrop::new(self);
        DmaObject {
            ptr: this.ptr.cast::<T>(),
            dma_addr: this.dma_addr,
            size: this.size,
            alloc: this.alloc,
        }
    }
}

/// A contiguous slice of DMA-capable memory.
///
/// Frees its backing memory via the stored allocator when dropped.
pub struct DmaSlice<'a, T> {
    ptr: NonNull<T>,
    dma_addr: DmaAddr,
    len: usize,  // Length in number of elements
    size: usize, // Length in bytes
    alloc: &'a dyn DmaAllocator,
}

// SAFETY: the memory `DmaSlice` points to is a permanent identity-mapped DMA allocation
unsafe impl<'a, T: Send> Send for DmaSlice<'a, T> {}
// SAFETY: the memory `DmaSlice` points to is a permanent identity-mapped DMA allocation
unsafe impl<'a, T: Sync> Sync for DmaSlice<'a, T> {}

impl<'a, T> DmaSlice<'a, T> {
    /// Creates a new `DmaSlice` from the given components.
    ///
    /// The slice will be freed via `alloc` when dropped.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid memory region of at least `len * size_of::<T>()` bytes, and
    /// `dma_addr` must correspond to the physical address of that memory region. The region
    /// must have been allocated by `alloc`.
    pub unsafe fn new_unchecked(
        ptr: NonNull<T>,
        dma_addr: DmaAddr,
        len: usize,
        size: usize,
        alloc: &'a dyn DmaAllocator,
    ) -> DmaSlice<'a, T> {
        DmaSlice {
            ptr,
            dma_addr,
            len,
            size,
            alloc,
        }
    }

    /// Returns the DMA-capable physical address of the slice.
    pub fn dma_addr(&self) -> DmaAddr {
        self.dma_addr
    }

    /// Returns the length of the slice in number of elements.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the slice is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the length of the slice in bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns a raw pointer to the slice.
    pub fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Returns a mutable raw pointer to the slice.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Returns the slice as a reference to a slice of `T`.
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: by construction, ptr points to a valid memory region of at least len * size_of::<T>() bytes
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// Returns the slice as a mutable reference to a slice of `T`.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: by construction, ptr points to a valid memory region of at least len * size_of::<T>() bytes
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    /// Synchronizes the slice for CPU access.
    ///
    /// See [`sync_for_cpu`](DmaAllocator::sync_for_cpu) for details.
    pub fn sync_for_cpu(&self, direction: DmaDirection) {
        self.alloc.sync_for_cpu(self.dma_addr, self.size, direction);
    }

    /// Synchronizes the slice for device access.
    ///
    /// See [`sync_for_device`](DmaAllocator::sync_for_device) for details.
    pub fn sync_for_device(&self, direction: DmaDirection) {
        self.alloc
            .sync_for_device(self.dma_addr, self.size, direction);
    }
}

impl<'a, T> Drop for DmaSlice<'a, T> {
    fn drop(&mut self) {
        let parts = DmaBufParts {
            ptr: self.ptr.cast::<u8>(),
            dma_addr: self.dma_addr,
            size: self.size,
            align: mem::align_of::<T>(),
        };
        // SAFETY: by construction, this slice was allocated by self.alloc
        unsafe {
            self.alloc.free_raw(parts);
        }
    }
}

impl<'a, T: DmaSafe> DmaSlice<'a, MaybeUninit<T>> {
    /// Assumes the slice has been initialized and returns a `DmaSlice<T>`.
    ///
    /// Ownership of the backing memory is transferred; [`Drop`] is not run on `self`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the memory has been properly initialized.
    pub unsafe fn assume_init(self) -> DmaSlice<'a, T> {
        let this = ManuallyDrop::new(self);
        DmaSlice {
            ptr: this.ptr.cast::<T>(),
            dma_addr: this.dma_addr,
            len: this.len,
            size: this.size,
            alloc: this.alloc,
        }
    }
}

// Token type to ensure that only the HAL code can create allocators.
pub(crate) struct DmaAllocatorToken(());

/// Returns a reference to the architecture-specific DMA allocator.
#[inline]
pub fn allocator() -> &'static dyn DmaAllocator {
    crate::arch::hal::mm::dma::allocator(DmaAllocatorToken(()))
}
