/*
 * RISC-V Linux-like runtime startup code.
 * Sets up the environment and calls main()
 */

    .section .text
    .global _start
    .type   _start, @function

    .equ SYS_exit, 1

_start:
    /* If you use a global pointer (gp) in userspace code, your linker script
       should define __global_pointer$. If not, this is harmless. */
    # .option push
    # .option norelax
    # la   gp, __global_pointer$
    # .option pop

    /* TODO: clear .bss */

    /* Extract argc, argv, envp from the stack */
    li      a0, 0
    li      a1, 0
    li      a2, 0

    /* Call __entry(argc, argv, envp) */
    call    __entry

    /* On return from __entry, exit with its return value */
    mv      a0, a0
    li      a7, SYS_exit
    ecall

    /* If exit returns, spin */
3:
    j       3b

    .size  _start, .-_start
