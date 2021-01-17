use super::trap::init_trap_vector;

#[no_mangle]
pub extern "C" fn arch_init() {
    init_trap_vector();
}
