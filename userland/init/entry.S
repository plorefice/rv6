    .section .text
    .globl _start

_start:
    # sys_write(1, msg, msg_len)
    li      a7, 0          # SYS_write
    li      a0, 1          # fd = 1 (stdout)
    la      a1, msg        # buf
    li      a2, 24         # len
    ecall

1:  j       1b             # loop forever

msg:
    .ascii  "Hello from user space!\n"
