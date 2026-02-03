    .section .text
    .global __syscall6
    .type   __syscall6, @function
__syscall6:
    # C args on entry:
    # a0 = n
    # a1 = arg0
    # a2 = arg1
    # a3 = arg2
    # a4 = arg3
    # a5 = arg4
    # a6 = arg5

    mv      a7, a0          # syscall number to a7
    mv      a0, a1          # arg0 to a0
    mv      a1, a2          # arg1 to a1
    mv      a2, a3          # arg2 to a2
    mv      a3, a4          # arg3 to a3
    mv      a4, a5          # arg4 to a4
    mv      a5, a6          # arg5 to a5
    ecall                   # make syscall
    ret                     # return to caller
