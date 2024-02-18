use core::{ptr, str};

extern "C" {
    #[linkage = "extern_weak"]
    static ksyms_offsets: *const usize;
    #[linkage = "extern_weak"]
    static ksyms_num_syms: *const usize;
    #[linkage = "extern_weak"]
    static ksyms_markers: *const usize;
    #[linkage = "extern_weak"]
    static ksyms_names: *const u8;
}

/// Looks up a kernel symbol by address and returns its name and offset, or `None` if no symbol
/// is found.
pub fn resolve_symbol(pc: usize) -> Option<(&'static str, usize)> {
    let markers = unsafe {
        assert!(!ksyms_markers.is_null());
        &*ptr::slice_from_raw_parts(ksyms_markers, get_symtab_len())
    };

    if let Some((i, base)) = lookup_symbol(pc) {
        let symbol_name_ptr = unsafe {
            assert!(!ksyms_names.is_null());
            &*ptr::slice_from_raw_parts(ksyms_names.add(markers[i]), markers[i + 1] - markers[i])
        };

        return Some((
            unsafe { str::from_utf8_unchecked(symbol_name_ptr) },
            pc - base,
        ));
    }

    None
}

/// Looks up an address and returns the symbol's index in the symbol table and its base address,
/// or `None` if no symbol is found.
fn lookup_symbol(pc: usize) -> Option<(usize, usize)> {
    let offsets = unsafe {
        assert!(!ksyms_offsets.is_null());
        &*ptr::slice_from_raw_parts(ksyms_offsets, get_symtab_len())
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
    unsafe {
        assert!(!ksyms_num_syms.is_null());
        ksyms_num_syms.read()
    }
}
