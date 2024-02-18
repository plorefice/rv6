/*
 * Linker script for QEMU virt machine
 */

OUTPUT_ARCH("riscv")
ENTRY(_start)

SECTIONS
{
  /* OpenSBI's fw_jump.bin firmware will look here for the entry point */
  . = ABSOLUTE(0x80200000);

  __start = .;

  . = ALIGN(4);
  .text : {
    /* Put reset handler first in .text section so it ends up as the entry point of the program */
    PROVIDE(__text_start = .);
    KEEP(*(.init));
    . = ALIGN(8);
    *(.trap) *(.trap*)
    *(.text*)
    PROVIDE(__text_end = .);
  }

  /* Constant data area */
  . = ALIGN(4);
  .rodata : {
    PROVIDE(__rodata_start = .);
    *(.rodata*)
    PROVIDE(__rodata_end = .);
  }

  /* Data section starts here - we page-align it so that we can properly setup the MMU later on */
  . = ALIGN(4K);

  PROVIDE(__data_start = .);

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
    . += 64K;
    PROVIDE(__stack_start = .);
  }

  PROVIDE(__data_end = .);

  __end = .;

  /DISCARD/ : {
    *(.eh_frame*)
  }
}
