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
/* Logical size of TrapFrame (must match Rust). */
.set TRAP_FRAME_SIZE, 280
/*
 * Bytes reserved on the stack for a trap frame.
 * Rounded up from TRAP_FRAME_SIZE so that sp stays 16-byte aligned
 * (RISC-V ABI) when calling into Rust.
 */
.set TRAP_FRAME_ALLOC, 288

/* sstatus register bits */
.set SSTATUS_SPP, (1 << 8)

/*
 * Save general-purpose registers into the trap frame at (sp).
 * sp and tp are saved later by the caller (TF_SP / TF_TP).
 */
.macro SAVE_GPRS
    sd ra, TF_RA(sp)
    sd gp, TF_GP(sp)
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
.endm

/*
    Entry point for S-mode traps.
    Saves registers, calls handle_exception, restores and srets.
*/
.global trap_entry
.align 4
trap_entry:
    /*
     * Detect if trap came from S-mode or U-mode based on sscratch.
     * - if sscratch is zero, trap came from S-mode
     * - if sscratch is non-zero, it contains the ThreadInfo pointer
     */
    csrrw tp, sscratch, tp
    bnez  tp, trap_from_user

trap_from_kernel:
    /*
     * Trap from S-mode: we are already on the correct kernel stack.
     * Nest the trap frame on the interrupted stack so we do not
     * overwrite the interrupted frames (kthreads, syscall path, idle).
     * Leave ThreadInfo untouched.
     */
    csrr  tp, sscratch
    csrw  sscratch, x0
    addi  sp, sp, -TRAP_FRAME_ALLOC
    SAVE_GPRS
    addi  s0, sp, TRAP_FRAME_ALLOC   /* interrupted sp */
    mv    s5, tp                     /* interrupted tp */
    j     save_special

trap_from_user:
    /* Trap from U-mode: switch to the process kernel stack. */
    sd    sp, TI_USER_SP_OFFSET(tp)
    ld    sp, TI_KERN_SP_OFFSET(tp)
    addi  sp, sp, -TRAP_FRAME_ALLOC
    SAVE_GPRS
    ld    s0, TI_USER_SP_OFFSET(tp)  /* user sp */
    csrr  s5, sscratch               /* user tp */
    csrw  sscratch, x0
    /* fall through */

save_special:
    /* Save special registers; s0 = interrupted sp, s5 = interrupted tp */
    csrr  s1, sstatus
    csrr  s2, sepc
    csrr  s3, stval
    csrr  s4, scause
    sd    s0, TF_SP(sp)
    sd    s1, TF_SSTATUS(sp)
    sd    s2, TF_SEPC(sp)
    sd    s3, TF_STVAL(sp)
    sd    s4, TF_SCAUSE(sp)
    sd    s5, TF_TP(sp)

    /*
     * For S-mode traps, re-load the interrupted frame pointer into s0 so
     * handle_exception's prologue links the Rust frames into the interrupted
     * kernel stack (needed for unwind_stack_frame). Skip for U-mode: TF_S0
     * holds the user s0, which is not a kernel frame.
     */
    andi  s1, s1, SSTATUS_SPP
    beqz  s1, 1f
    ld    s0, TF_S0(sp)
1:

    /* Call trap-handling function in Rust code */
    mv    a0, sp
    mv    a1, tp
    jal   handle_exception

    /* Execution will fall through to ret_from_exception */

.global ret_from_exception
ret_from_exception:
    /* Check if we are returning to U-mode */
    ld    s0, TF_SSTATUS(sp)
    andi  s0, s0, SSTATUS_SPP       /* SPP */
    bnez  s0, restore_context       /* returning to S-mode, don't swap */

    /* Save unwound kernel stack pointer in thread info */
    addi  s0, sp, TRAP_FRAME_ALLOC
    sd    s0, TI_KERN_SP_OFFSET(tp)

    /* Write back thread info into sscratch so that we can retrieve it next time */
    csrw  sscratch, tp

restore_context:
    /* Restore sstatus and sepc to resume execution */
    ld    a0, TF_SSTATUS(sp)
    ld    a2, TF_SEPC(sp)
    csrw  sstatus, a0
    csrw  sepc, a2

    /* Restore GP registers */
    ld    ra, TF_RA(sp)
    /* sp is restored further down */
    ld    gp, TF_GP(sp)
    ld    tp, TF_TP(sp)
    ld    t0, TF_T0(sp)
    ld    t1, TF_T1(sp)
    ld    t2, TF_T2(sp)
    ld    s0, TF_S0(sp)
    ld    s1, TF_S1(sp)
    ld    a0, TF_A0(sp)
    ld    a1, TF_A1(sp)
    ld    a2, TF_A2(sp)
    ld    a3, TF_A3(sp)
    ld    a4, TF_A4(sp)
    ld    a5, TF_A5(sp)
    ld    a6, TF_A6(sp)
    ld    a7, TF_A7(sp)
    ld    s2, TF_S2(sp)
    ld    s3, TF_S3(sp)
    ld    s4, TF_S4(sp)
    ld    s5, TF_S5(sp)
    ld    s6, TF_S6(sp)
    ld    s7, TF_S7(sp)
    ld    s8, TF_S8(sp)
    ld    s9, TF_S9(sp)
    ld    s10, TF_S10(sp)
    ld    s11, TF_S11(sp)
    ld    t3, TF_T3(sp)
    ld    t4, TF_T4(sp)
    ld    t5, TF_T5(sp)
    ld    t6, TF_T6(sp)

    /* Restore interrupted stack pointer */
    ld    sp, TF_SP(sp)

    /* Back to normal execution */
    sret

/*
    Resume a process by switching page tables and restoring a trap frame.
    a0 = satp value
    a1 = pointer to TrapFrame (heap-resident)
    a2 = thread info virtual address
    a3 = kernel stack pointer
*/
.global resume_context
.align 4
resume_context:
    mv t5, a1

    csrw satp, a0
    sfence.vma
    fence.i

    sd a3, TI_KERN_SP_OFFSET(a2)
    csrw sscratch, a2

    ld t0, TF_SSTATUS(t5)
    ld t1, TF_SEPC(t5)
    csrw sstatus, t0
    csrw sepc, t1

    ld ra, TF_RA(t5)
    ld gp, TF_GP(t5)
    ld tp, TF_TP(t5)
    ld t0, TF_T0(t5)
    ld t1, TF_T1(t5)
    ld t2, TF_T2(t5)
    ld s0, TF_S0(t5)
    ld s1, TF_S1(t5)
    ld a0, TF_A0(t5)
    ld a4, TF_A4(t5)
    ld a5, TF_A5(t5)
    ld a6, TF_A6(t5)
    ld a7, TF_A7(t5)
    ld s2, TF_S2(t5)
    ld s3, TF_S3(t5)
    ld s4, TF_S4(t5)
    ld s5, TF_S5(t5)
    ld s6, TF_S6(t5)
    ld s7, TF_S7(t5)
    ld s8, TF_S8(t5)
    ld s9, TF_S9(t5)
    ld s10, TF_S10(t5)
    ld s11, TF_S11(t5)
    ld t3, TF_T3(t5)
    ld t4, TF_T4(t5)
    ld t6, TF_T6(t5)
    ld a2, TF_A2(t5)
    ld a3, TF_A3(t5)
    ld a1, TF_A1(t5)
    ld sp, TF_SP(t5)
    ld t5, TF_T5(t5)

    sret
