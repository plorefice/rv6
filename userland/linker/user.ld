/* Linker script for RISC-V RV64 userland programs */

PAGE_SIZE = 0x1000;

ENTRY(_start)

SECTIONS {
    . = 0x40000000; /* Userland base address */

    . = ALIGN(PAGE_SIZE);
    .text : { *(.text .text.*) }

    . = ALIGN(PAGE_SIZE);
    .rodata : { *(.rodata .rodata.*) }

    . = ALIGN(PAGE_SIZE);
    .data : { *(.data .data.*) }

    . = ALIGN(PAGE_SIZE);
    .bss : { *(.bss .bss.* COMMON) }
}
