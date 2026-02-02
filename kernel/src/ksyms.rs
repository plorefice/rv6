//! Access to kernel symbols for debugging.

use core::{ffi::c_uchar, ptr, str};

unsafe extern "C" {
    static ksyms_offsets: usize; // actually an array
    static ksyms_num_syms: usize;
    static ksyms_markers: usize; // actually an array
    static ksyms_names: c_uchar; // actually an array
}

/// Looks up a kernel symbol by address and returns its name and offset, or `None` if no symbol
/// is found.
pub fn resolve_symbol(pc: usize) -> Option<(&'static str, usize)> {
    // SAFETY: safe because we first check that ksyms_markers is a valid pointer
    let markers = unsafe {
        let ptr = &ksyms_markers as *const usize;
        assert!(!ptr.is_null());
        &*ptr::slice_from_raw_parts(ptr, get_symtab_len())
    };

    if let Some((i, base)) = lookup_symbol(pc) {
        // SAFETY: safe because we first check that ksyms_names is a valid pointer
        let symbol_name_ptr = unsafe {
            let ptr = &ksyms_names as *const c_uchar;
            assert!(!ptr.is_null());
            &*ptr::slice_from_raw_parts(ptr.add(markers[i]), markers[i + 1] - markers[i])
        };

        return Some((
            // SAFETY: safe because symbol_name_ptr is guaranteed to be valid UTF-8
            unsafe { str::from_utf8_unchecked(symbol_name_ptr) },
            pc - base,
        ));
    }

    None
}

/// Looks up an address and returns the symbol's index in the symbol table and its base address,
/// or `None` if no symbol is found.
fn lookup_symbol(pc: usize) -> Option<(usize, usize)> {
    // SAFETY: safe because we first check that ksyms_offsets is a valid pointer
    let offsets = unsafe {
        let ptr = &ksyms_offsets as *const usize;
        assert!(!ptr.is_null());
        &*ptr::slice_from_raw_parts(ptr, get_symtab_len())
    };

    for (i, (&a, &b)) in offsets.iter().zip(offsets.iter().skip(1)).enumerate() {
        if (a..b).contains(&pc) {
            return Some((i, a));
        }
    }

    None
}

/// Returns the number of symbols in the table.
fn get_symtab_len() -> usize {
    // SAFETY: safe
    unsafe { ksyms_num_syms }
}
