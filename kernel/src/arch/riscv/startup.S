/* Boot code for a single-core application */

.section .init, "ax"
.global _start
_start:
    // Mask all interrupts
    csrw sie, zero
    csrw sip, zero

#if CONFIG_QEMU
    // If hart ID is not #0, park it
    bnez a0, park_core
#endif

    // Save DTB pointer, will be later passed to arch_init()
    mv s1, a1

.option push
.option norelax
    la gp, __global_pointer$
.option pop

    // Clear BSS section
    la a3, __bss_start
    la a4, __bss_end
    ble a4, a3, clear_bss_done
clear_bss:
    sd zero, (a3)
    add a3, a3, 8
    blt a3, a4, clear_bss
clear_bss_done:

    // Allocate stack
    la sp, __stack_start

    // Setup dummy stack frame
    addi sp, sp, -16
    sd zero, 8(sp)
    sd zero, 0(sp)
    addi s0, sp, 16

    // Disable paging
    csrwi satp, 0

    // Jump to Rust code
    mv a0, s1
    call arch_init
    tail kmain

park_core:
    wfi
    j park_core
