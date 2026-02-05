//! Interfaces for DMA-capable memory addresses and allocation.

use core::{
    alloc::Layout,
    mem::{self, MaybeUninit},
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

/// A trait for DMA-capable memory allocators.
///
/// Provides methods for allocating and freeing memory regions that can be used
/// for DMA operations, as well as synchronizing memory between the CPU and devices.
pub trait DmaAllocator: Send + Sync {
    /// Allocates a DMA-capable memory region with the specified layout.
    fn alloc_raw(&self, layout: Layout) -> Result<DmaBuf, DmaAllocError>;

    /// Allocates a DMA-capable memory region with the specified layout, initialized to zero.
    fn alloc_raw_zeroed(&self, layout: Layout) -> Result<DmaBuf, DmaAllocError> {
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
    /// The provided buffer must have been allocated by this allocator.
    unsafe fn free_raw(&self, buf: DmaBuf);

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
/// Provides higher-level allocation methods for typed objects.
pub trait DmaAllocatorExt: DmaAllocator {
    /// Allocates a DMA-capable memory region for an object of type `T`.
    ///
    /// The provided value is copied into the allocated region. If you don't need to initialize
    /// the memory, consider using [`alloc_uninit`] or [`alloc_zeroed`] instead
    fn alloc<T: DmaSafe>(&self, val: T) -> Result<DmaObject<T>, DmaAllocError> {
        let mut obj = self.alloc_uninit::<T>()?;
        // SAFETY: obj points to at least size_of::<T>() bytes, properly mapped for CPU access.
        unsafe {
            ptr::write(obj.as_mut_ptr().cast::<T>(), val);
        }
        // SAFETY: by construction we just initialized all bytes of T.
        Ok(unsafe { obj.assume_init() })
    }

    /// Allocates a DMA-capable memory region for an uninitialized object of type `T`.
    fn alloc_uninit<T: DmaSafe>(&self) -> Result<DmaObject<MaybeUninit<T>>, DmaAllocError> {
        let layout = Layout::new::<T>();

        // Allocate enough frames to cover the requested layout
        let buf = self.alloc_raw(layout)?;

        // SAFETY: `buf.ptr` is known to be a valid pointer
        let ptr = unsafe { NonNull::new_unchecked(buf.ptr.as_ptr() as *mut MaybeUninit<T>) };

        // SAFETY: by construction
        Ok(unsafe { DmaObject::new_unchecked(ptr, buf.dma_addr, buf.size) })
    }

    /// Allocates a DMA-capable memory region for an object of type `T`, initializing it to zero.
    fn alloc_zeroed<T: DmaSafe>(&self) -> Result<DmaObject<T>, DmaAllocError> {
        let mut obj = self.alloc_uninit::<T>()?;
        // SAFETY: obj points to at least size_of::<T>() bytes, properly mapped for CPU access.
        unsafe {
            ptr::write_bytes(obj.as_mut_ptr().cast::<u8>(), 0, mem::size_of::<T>());
        }
        // SAFETY: by construction we just initialized all bytes of T to zero.
        Ok(unsafe { obj.assume_init() })
    }

    /// Frees a previously allocated DMA-capable memory region.
    fn free<T>(&self, obj: DmaObject<T>) {
        let buf = DmaBuf {
            ptr: obj.ptr.cast::<u8>(),
            dma_addr: obj.dma_addr,
            size: obj.size,
            align: mem::align_of::<T>(),
        };
        // SAFETY: by construction, obj was allocated by this allocator
        unsafe {
            self.free_raw(buf);
        }
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
#[derive(Debug)]
pub struct DmaBuf {
    ptr: NonNull<u8>,
    dma_addr: DmaAddr,
    size: usize,
    align: usize,
}

impl DmaBuf {
    /// Creates a new `DmaBuf` from the given components.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid memory region of at least `size` bytes, and `dma_addr` must
    /// correspond to the physical address of that memory region.
    pub unsafe fn new_unchecked(
        ptr: NonNull<u8>,
        dma_addr: DmaAddr,
        size: usize,
        align: usize,
    ) -> DmaBuf {
        DmaBuf {
            ptr,
            dma_addr,
            size,
            align,
        }
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
}

/// An object allocated in DMA-capable memory.
pub struct DmaObject<T> {
    ptr: NonNull<T>,   // CPU accessible pointer
    dma_addr: DmaAddr, // Device-visible physical address
    size: usize,       // Length in bytes
}

impl<T> DmaObject<T> {
    /// Creates a new `DmaObject` from the given components.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a valid memory region of at least `size` bytes, and `dma_addr` must
    /// correspond to the physical address of that memory region.
    pub unsafe fn new_unchecked(ptr: NonNull<T>, dma_addr: DmaAddr, size: usize) -> DmaObject<T> {
        DmaObject {
            ptr,
            dma_addr,
            size,
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
}

impl<T: DmaSafe> DmaObject<MaybeUninit<T>> {
    /// Assumes the object has been initialized and returns a `DmaObject<T>`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the memory has been properly initialized.
    pub unsafe fn assume_init(self) -> DmaObject<T> {
        DmaObject {
            ptr: self.ptr.cast::<T>(),
            dma_addr: self.dma_addr,
            size: self.size,
        }
    }
}

// Token type to ensure that only the HAL code can create allocators.
pub(crate) struct DmaAllocatorToken(());

/// Returns a reference to the architecture-specific DMA allocator.
#[inline]
pub fn allocator() -> &'static impl DmaAllocator {
    crate::arch::hal::mm::dma::allocator(DmaAllocatorToken(()))
}
