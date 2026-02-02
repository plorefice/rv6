.global _start
_start:
    /* ECALL $42 */
    li a0, 0
    li a7, 42
    ecall
    j .
