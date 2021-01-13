/// Writes a byte to the memory location `base + offset`.
///
/// # Safety
///
/// This function is _highly_ unsafe, in the sense that we cannot make any
/// guarantees about the memory location that is going to be written.
pub unsafe fn write_u8(base: usize, offset: usize, val: u8) {
    let reg = base as *mut u8;
    reg.add(offset).write_volatile(val);
}
