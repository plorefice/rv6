/*
    Kernel-mode context switch code for RISC-V architecture.
*/

/* Offsets in the Context structure */
.set CTX_RA, 0
.set CTX_SP, 8
.set CTX_S0, 16
.set CTX_S1, 24
.set CTX_S2, 32
.set CTX_S3, 40
.set CTX_S4, 48
.set CTX_S5, 56
.set CTX_S6, 64
.set CTX_S7, 72
.set CTX_S8, 80
.set CTX_S9, 88
.set CTX_S10, 96
.set CTX_S11, 104

/*
    Save the current context into *old, switch address space / tp, then
    restore the context from *new and return into it.

    fn swtch(old: *mut Context, new: *const Context, satp: usize, tp: usize)

    Arguments:
      a0: pointer to old context (Context*)
      a1: pointer to new context (Context*)
      a2: satp value for the incoming address space
      a3: thread-info pointer for the incoming task (loaded into tp)

    satp/tp must change *after* saving the old sp and *before* loading the
    new sp: each process maps its private kernel stack at the same VA.
*/
.global swtch
.align 4
swtch:
    /* Save callee-saved registers into the old context */
    sd ra, CTX_RA(a0)
    sd sp, CTX_SP(a0)
    sd s0, CTX_S0(a0)
    sd s1, CTX_S1(a0)
    sd s2, CTX_S2(a0)
    sd s3, CTX_S3(a0)
    sd s4, CTX_S4(a0)
    sd s5, CTX_S5(a0)
    sd s6, CTX_S6(a0)
    sd s7, CTX_S7(a0)
    sd s8, CTX_S8(a0)
    sd s9, CTX_S9(a0)
    sd s10, CTX_S10(a0)
    sd s11, CTX_S11(a0)

    /* Install the incoming address space and thread info */
    csrw satp, a2
    sfence.vma
    fence.i
    mv tp, a3
    /* In-kernel invariant: sscratch == 0 (see trap_entry) */
    csrw sscratch, x0

    /* Load callee-saved registers from the new context */
    ld ra, CTX_RA(a1)
    ld sp, CTX_SP(a1)
    ld s0, CTX_S0(a1)
    ld s1, CTX_S1(a1)
    ld s2, CTX_S2(a1)
    ld s3, CTX_S3(a1)
    ld s4, CTX_S4(a1)
    ld s5, CTX_S5(a1)
    ld s6, CTX_S6(a1)
    ld s7, CTX_S7(a1)
    ld s8, CTX_S8(a1)
    ld s9, CTX_S9(a1)
    ld s10, CTX_S10(a1)
    ld s11, CTX_S11(a1)

    /* Return into the new context (ra restored above) */
    ret
