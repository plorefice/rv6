/*
 * Linker script for QEMU virt machine
 */

OUTPUT_ARCH("riscv")
ENTRY(_start)

SECTIONS
{
  /* OpenSBI's fw_jump.bin firmware will look here for the entry point */
  . = ABSOLUTE(0x80200000);

  . = ALIGN(4);
  .text : {
    /* Put reset handler first in .text section so it ends up as the entry point of the program */
    KEEP(*(.init));
    *(.trap*)
    *(.text*)
  }

  /* Constant data area */
  . = ALIGN(4);
  .rodata : {
    *(.rodata*)
  }

  /* Small data area - RISC-V provides optimized access to this area */
  . = ALIGN(4);
  .sdata : {
    /* Point the global pointer halfway through this section */ 
    PROVIDE(__global_pointer$ = . + (4K / 2));
    *(.sdata*)
  }

  /* Regular data area */
  . = ALIGN(4);
  .data : {
    *(.data*)
  }

  /* Zero-initialized data */
  . = ALIGN(4);
  .bss : {
    PROVIDE(__bss_start = .);
    *(.sbss*) *(.bss*)
    . = ALIGN(8);
    PROVIDE(__bss_end = .);
  }

  /* Reserved stack space for startup code */
  . = ALIGN(16);
  .stack : {
    PROVIDE(__stack_end = .);
    . += 4K;
    PROVIDE(__stack_start = .);
  }
}
