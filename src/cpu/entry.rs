use crate::kmain;

use super::trap::init_trap_vector;

#[no_mangle]
pub extern "C" fn start() -> ! {
    init_trap_vector();

    // Jump to non-arch-specific kernel entry point
    kmain();
}
