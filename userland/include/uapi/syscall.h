#ifndef UAPI_SYSCALL_H
#define UAPI_SYSCALL_H

/*
 * System call numbers for the operating system.
 * This file is part of the userland API.
 */
#define __NR_write 0

/* Raw system call prototypes */
long __syscall6(long n, long a0, long a1, long a2, long a3, long a4, long a5);

static inline long __syscall0(long n)
{
    return __syscall6(n, 0, 0, 0, 0, 0, 0);
}

static inline long __syscall1(long n, long a0)
{
    return __syscall6(n, a0, 0, 0, 0, 0, 0);
}

static inline long __syscall2(long n, long a0, long a1)
{
    return __syscall6(n, a0, a1, 0, 0, 0, 0);
}

static inline long __syscall3(long n, long a0, long a1, long a2)
{
    return __syscall6(n, a0, a1, a2, 0, 0, 0);
}

static inline long __syscall4(long n, long a0, long a1, long a2, long a3)
{
    return __syscall6(n, a0, a1, a2, a3, 0, 0);
}

static inline long __syscall5(long n, long a0, long a1, long a2, long a3, long a4)
{
    return __syscall6(n, a0, a1, a2, a3, a4, 0);
}

#endif /* UAPI_SYSCALL_H */
