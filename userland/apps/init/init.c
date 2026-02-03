extern long write(int fd, const void *buf, unsigned long count);

int main(void)
{
    const char *msg = "Hello, RV6!\n";
    write(1, msg, 13);

    for (;;)
        ;
}
