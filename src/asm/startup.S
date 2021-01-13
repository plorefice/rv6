/* Boot code for a single-core application */

.section .init, "ax"
.global _start
_start:
    // If hart ID is not #0, park it
    csrr t0, mhartid
    bnez t0, 3f

.option push
.option norelax
    la gp, __global_pointer$
.option pop

    // Clear .bss
    la a0, _sbss
    la a1, _ebss
    bgeu a0, a1, 2f
1:
    sw zero, (a0)
    addi a0, a0, 4
    bltu a0, a1, 1b
2:
    // Allocate stack
    la sp, _stack_start
    lui t0, %hi(_stack_size)
    add t0, t0, %lo(_stack_size)
    sub sp, sp, t0

    // Set frame pointer
    add s0, sp, zero

    // Jump to Rust code
    la t1, kmain
    la ra, 3f
    csrw mepc, t1
    mret
3:
    wfi
    j 3b