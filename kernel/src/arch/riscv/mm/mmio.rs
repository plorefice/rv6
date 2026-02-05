use core::{
    alloc::{GlobalAlloc, Layout},
    num::NonZeroUsize,
};

use crate::{
    arch::riscv::{
        mm::{GFA, IOMAP, MAPPER},
        mmu::{EntryFlags, PAGE_SIZE, PageSize},
    },
    mm::{
        addr::{Align, MemoryAddress, PhysAddr, VirtAddr},
        mmio::{IoMapError, IoMapper, IoMapping},
    },
};

#[derive(Debug)]
pub struct RiscvIoMapper;

impl IoMapper for RiscvIoMapper {
    fn iomap(&self, base: PhysAddr, len: NonZeroUsize) -> Result<IoMapping, IoMapError> {
        // TODO: add more validations here
        let pa_start = base.align_down(PAGE_SIZE);
        let pa_end = (base + len.get()).align_up(PAGE_SIZE);
        let map_len = (pa_end - pa_start).as_usize();

        // Reserve page-aligned virtual memory for the mapping
        let layout = Layout::from_size_align(map_len, PAGE_SIZE).expect("invalid memory layout");
        // SAFETY: the layout is valid
        let va_map = unsafe { IOMAP.alloc(layout) };
        if va_map.is_null() {
            return Err(IoMapError::MappingFailed);
        }

        let va_map = VirtAddr::new(va_map as usize);

        // SAFETY: all checks are in place
        unsafe {
            MAPPER
                .lock()
                .as_mut()
                .expect("no mapper?")
                .map_range(
                    va_map,
                    pa_start..pa_end,
                    PageSize::Kb,
                    EntryFlags::MMIO,
                    GFA.lock().as_mut().unwrap(),
                )
                .unwrap();
        }

        // SAFETY: by construction
        Ok(unsafe { IoMapping::new_unchecked(va_map, len, base) })
    }

    unsafe fn iounmap(&self, mapping: IoMapping) {
        todo!()
    }
}
