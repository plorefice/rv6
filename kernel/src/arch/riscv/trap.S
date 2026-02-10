/*
    Trap entry points for S-mode
 */

/* Global constants for trap handling */

/* Offsets in the thread info struct */
.set TI_KERN_SP_OFFSET, 0
.set TI_USER_SP_OFFSET, 8

/* Offsets in the trap frame */
.set TF_RA, 0
.set TF_SP, 8
.set TF_GP, 16
.set TF_TP, 24
.set TF_T0, 32
.set TF_T1, 40
.set TF_T2, 48
.set TF_S0, 56
.set TF_S1, 64
.set TF_A0, 72
.set TF_A1, 80
.set TF_A2, 88
.set TF_A3, 96
.set TF_A4, 104
.set TF_A5, 112
.set TF_A6, 120
.set TF_A7, 128
.set TF_S2, 136
.set TF_S3, 144
.set TF_S4, 152
.set TF_S5, 160
.set TF_S6, 168
.set TF_S7, 176
.set TF_S8, 184
.set TF_S9, 192
.set TF_S10, 200
.set TF_S11, 208
.set TF_T3, 216
.set TF_T4, 224
.set TF_T5, 232
.set TF_T6, 240
.set TF_SSTATUS, 248
.set TF_SEPC, 256
.set TF_STVAL, 264
.set TF_SCAUSE, 272
.set TRAP_FRAME_SIZE, 280

/* sstatus register bits */
.set SSTATUS_SPP, (1 << 8)

/*
    Entry point for S-mode traps (_strap_entry)
    Saves registers, calls _do_handle_strap_rust,
    restores caller saved registers and then returns.
*/
.global trap_entry
.align 4
trap_entry:
    /*
     * Detect if trap came from S-mode or U-mode based on sscratch.
     * - if sscratch is zero, trap came from S-mode
     * - if sscratch is non-zero, it contains the pointer to the ProcessLocalStorage
     */
    csrrw tp, sscratch, tp
    bnez  tp, save_context

restore_kernel:
    /* Trap came from kernel: restore TP to the kernel thread info struct */
    csrr  tp, sscratch

save_context:
    sd sp, TI_USER_SP_OFFSET(tp) // Save user stack pointer
    ld sp, TI_KERN_SP_OFFSET(tp) // Load kernel stack pointer

    /* Allocate space for trap frame */
    addi sp, sp, -TRAP_FRAME_SIZE

    /* Save GP registers */
    sd ra, TF_RA(sp)
    /* sp is saved later */
    sd gp, TF_GP(sp)
    /* tp is saved later */
    sd t0, TF_T0(sp)
    sd t1, TF_T1(sp)
    sd t2, TF_T2(sp)
    sd s0, TF_S0(sp)
    sd s1, TF_S1(sp)
    sd a0, TF_A0(sp)
    sd a1, TF_A1(sp)
    sd a2, TF_A2(sp)
    sd a3, TF_A3(sp)
    sd a4, TF_A4(sp)
    sd a5, TF_A5(sp)
    sd a6, TF_A6(sp)
    sd a7, TF_A7(sp)
    sd s2, TF_S2(sp)
    sd s3, TF_S3(sp)
    sd s4, TF_S4(sp)
    sd s5, TF_S5(sp)
    sd s6, TF_S6(sp)
    sd s7, TF_S7(sp)
    sd s8, TF_S8(sp)
    sd s9, TF_S9(sp)
    sd s10, TF_S10(sp)
    sd s11, TF_S11(sp)
    sd t3, TF_T3(sp)
    sd t4, TF_T4(sp)
    sd t5, TF_T5(sp)
    sd t6, TF_T6(sp)

    /* Save special registers */
    ld   s0, TI_USER_SP_OFFSET(tp)    // user sp
    csrr s1, sstatus
	csrr s2, sepc
	csrr s3, stval
	csrr s4, scause
	csrr s5, sscratch // tp
	sd   s0, TF_SP(sp)
	sd   s1, TF_SSTATUS(sp)
	sd   s2, TF_SEPC(sp)
	sd   s3, TF_STVAL(sp)
	sd   s4, TF_SCAUSE(sp)
	sd   s5, TF_TP(sp)

 	/*
	 * Set the scratch register to 0, so that if a recursive exception
	 * occurs, the exception vector knows it came from the kernel
	 */
	csrw sscratch, x0

    /* Call trap-handling function in Rust code */
	mv a0, sp
    jal handle_exception

    /* Execution will fall through to ret_from_exception */

.global ret_from_exception
ret_from_exception:
    /* Check if we are returning to U-mode */
    ld s0, TF_SSTATUS(sp)
    andi s0, s0, SSTATUS_SPP       // SPP
    bnez s0, restore_context    // returning to S-mode, don't swap

 	/* Save unwound kernel stack pointer in thread info */
	addi s0, sp, TRAP_FRAME_SIZE
	sd   s0, TI_KERN_SP_OFFSET(tp)

    /* Write back thread info into sscratch so that we can retrieve it next time */
    csrw sscratch, tp

restore_context:
    /* Restore sstatus and sepc to resume U-mode execution */
    ld a0, TF_SSTATUS(sp)
    ld a2, TF_SEPC(sp)
    csrw sstatus, a0
    csrw sepc, a2

    /* Restore GP registers */
    ld ra, TF_RA(sp)
    /* sp is restored further down */
    ld gp, TF_GP(sp)
    ld tp, TF_TP(sp)
    ld t0, TF_T0(sp)
    ld t1, TF_T1(sp)
    ld t2, TF_T2(sp)
    ld s0, TF_S0(sp)
    ld s1, TF_S1(sp)
    ld a0, TF_A0(sp)
    ld a1, TF_A1(sp)
    ld a2, TF_A2(sp)
    ld a3, TF_A3(sp)
    ld a4, TF_A4(sp)
    ld a5, TF_A5(sp)
    ld a6, TF_A6(sp)
    ld a7, TF_A7(sp)
    ld s2, TF_S2(sp)
    ld s3, TF_S3(sp)
    ld s4, TF_S4(sp)
    ld s5, TF_S5(sp)
    ld s6, TF_S6(sp)
    ld s7, TF_S7(sp)
    ld s8, TF_S8(sp)
    ld s9, TF_S9(sp)
    ld s10, TF_S10(sp)
    ld s11, TF_S11(sp)
    ld t3, TF_T3(sp)
    ld t4, TF_T4(sp)
    ld t5, TF_T5(sp)
    ld t6, TF_T6(sp)

    /* Restore user stack pointer */
    ld sp, TF_SP(sp)

    /* Back to normal execution */
    sret