#include <uapi/syscall.h>

long write(int fd, const void *buf, unsigned long count)
{
    return __syscall3(__NR_write, (long)fd, (long)buf, (long)count);
}
