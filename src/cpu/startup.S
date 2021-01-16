/* Boot code for a single-core application */

.section .init, "ax"
.global _start
_start:
    // Mask all interrupts
    csrw mie, zero
    csrw mip, zero

    call reset_regs

    // If hart ID is not #0, park it
    csrr a0, mhartid
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
    la sp, _stack_start
    add s0, sp, zero

    // Prepare to jump to S-mode by setting mstatus[MPP] to Supervisor
    csrr t0, mstatus
    li t1, (0b01 << 11)
    or t0, t0, t1
    csrw mstatus, t0

    // Load start() symbol address to jump to Rust code
    la t0, start
    csrw mepc, t0

    // Disable paging
    csrwi satp, 0

    // Delegate all interrupt and exception handling to S-mode
    li t0, 0xffff
    csrw mideleg, t0
    csrw medeleg, t0

    csrr t0, sie
    ori t0, t0, (1 << 9) | (1 << 5) | (1 << 1)
    csrw sie, t0

    // Jump to Rust code
    la ra, 3f
    mret
3:
    wfi
    j 3b

// Clear all registers except ra, a0 and a1
reset_regs:
	li sp, 0
	li gp, 0
	li tp, 0
	li t0, 0
	li t1, 0
	li t2, 0
	li s0, 0
	li s1, 0
	li a2, 0
	li a3, 0
	li a4, 0
	li a5, 0
	li a6, 0
	li a7, 0
	li s2, 0
	li s3, 0
	li s4, 0
	li s5, 0
	li s6, 0
	li s7, 0
	li s8, 0
	li s9, 0
	li s10, 0
	li s11, 0
	li t3, 0
	li t4, 0
	li t5, 0
	li t6, 0
	csrw mscratch, 0
    ret