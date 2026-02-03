ENTRY(_start)

SECTIONS {
    . = 0x40000000;

    .text : {
        *(.text*)
    }

    .rodata : {
        *(.rodata*)
    }

    . = 0x40001000;

    .data : {
        *(.data*)
    }

    .bss : {
        *(.bss*)
        *(COMMON)
    }
}
