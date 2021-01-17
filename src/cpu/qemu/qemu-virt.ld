/*
 * Linker script for QEMU virt machine
 */

/* Target architecture (both RV32 and RV64) */
OUTPUT_ARCH("riscv")

/* Memory layout for qemu-system-risc32 */
MEMORY
{
  RAM (rwx) : ORIGIN = 0x80200000, LENGTH = 128M
}

/* Place stack at the end of the RAM region */
PROVIDE(_stack_size = 4K);
PROVIDE(_stack_start = ORIGIN(RAM) + LENGTH(RAM));
PROVIDE(_stack_end = _stack_start - _stack_size);

/* Program entry point */
ENTRY(_start)

SECTIONS
{
  .text :
  {
    /* Put reset handler first in .text section so it ends up as the entry */
    /* point of the program. */
    KEEP(*(.init));
    . = ALIGN(4);
    (*(.trap));
    (*(.trap.rust));
  
    *(.text .text.*);
  } > RAM

  .rodata : ALIGN(4)
  {
    *(.rodata .rodata.*);
    /* 4-byte align the end (VMA) of this section.
       This is required by LLD to ensure the LMA of the following .data
       section will have the correct alignment. */
    . = ALIGN(4);
  } > RAM

  .data : ALIGN(4)
  {
    __data_start = .;
    /* Must be called __global_pointer$ for linker relaxations to work. */
    PROVIDE(__global_pointer$ = . + 0x800);
    *(.sdata .sdata.* .sdata2 .sdata2.*);
    *(.data .data.*);
    . = ALIGN(4);
    __data_end = .;
  } > RAM

  .bss (NOLOAD) : ALIGN(4)
  {
    __bss_start = .;
    *(.bss .bss.*);
    . = ALIGN(4);
    __bss_end = .;
  } > RAM

  /* fictitious region that represents the memory available for the stack */
  .stack (NOLOAD) : ALIGN(16)
  {
    _estack = .;
    . = ABSOLUTE(_stack_end);
    . += _stack_size;
    _sstack = .;
  } > RAM

  .eh_frame (INFO) : { KEEP(*(.eh_frame)) }
  .eh_frame_hdr (INFO) : { *(.eh_frame_hdr) }
}

/* Do not exceed this mark in the error messages above                                    | */
ASSERT(ORIGIN(RAM) % 4 == 0, "
ERROR(rx6): the start of the RAM must be 4-byte aligned");

ASSERT(__data_start % 4 == 0 && __data_end % 4 == 0, "
BUG(rx6): .data is not 4-byte aligned");

ASSERT(__bss_start % 4 == 0 && __bss_end % 4 == 0, "
BUG(rx6): .bss is not 4-byte aligned");

/* Do not exceed this mark in the error messages above                                    | */