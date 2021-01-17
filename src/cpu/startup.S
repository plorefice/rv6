/* Boot code for a single-core application */

.section .init, "ax"
.global _start
_start:
    // Mask all interrupts
    csrw sie, zero
    csrw sip, zero

    // If hart ID is not #0, park it
    bnez a0, 3f

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
    add s0, sp, zero

    // Disable paging
    csrwi satp, 0

    // Jump to Rust code
    call start
3:
    wfi
    j 3b
