/*
 * Linker script for QEMU virt machine
 *
 * Based on Linux kernel's arch/riscv/kernel/vmlinux.lds.S.
 */

/* NOTE: must be kept in sync with head.S */
LOAD_OFFSET = 0xffffffe000000000;
PAGE_ALIGN = (1 << 12);
SECTION_ALIGN = (1 << 21);

OUTPUT_ARCH("riscv")
ENTRY(_start)

SECTIONS
{
  . = LOAD_OFFSET;

  _start = .;

  .head.text : AT(ADDR(.head.text) - LOAD_OFFSET) {
    KEEP(*(.head.text))
  }

  . = ALIGN(PAGE_ALIGN);
  .init.text : AT(ADDR(.init.text) - LOAD_OFFSET) {
    *(.init.text .init.text.*)
  }

  . = ALIGN(SECTION_ALIGN);
  .text : {
    _stext = .;
    *(.text .text.*)
    _etext = .;
  }

  . = ALIGN(SECTION_ALIGN);
  _srodata = .;

  .rodata : AT(ADDR(.rodata) - LOAD_OFFSET) {
    *(.rodata .rodata.*)
  }

  . = ALIGN(SECTION_ALIGN);
  .srodata : {
    *(.rodata .rodata.*)
  }

  _erodata = .;

  . = ALIGN(SECTION_ALIGN);
  _sdata = .;

  .data : AT(ADDR(.data) - LOAD_OFFSET) {
    _estack = .;
    . += 64K;
    _sstack = .;

    . = ALIGN(PAGE_ALIGN);
    *(.data..page_aligned)

    . = ALIGN(PAGE_ALIGN);
    *(.data)
  }

  .sdata : {
    __global_pointer$ = . + 0x800;
    *(.sdata*)
  }

  _edata = .;

  . = ALIGN(PAGE_ALIGN);
  _sbss = .;

  .sbss : AT(ADDR(.sbss) - LOAD_OFFSET) {
    *(.dynsbss) *(.sbss) *(.scommon)
  }

  . = ALIGN(PAGE_ALIGN);
  .bss : AT(ADDR(.bss) - LOAD_OFFSET) {
    *(.dynbss) *(.bss) *(COMMON)
  }

  _ebss = .;

  .rel.dyn : {
    *(.rel.dyn*)
  }

  _end = .;

  /DISCARD/ : {
    *(.eh_frame*)
  }
}
