/*
 * Entry point for the RISC-V architecture.
 */

/* NOTE: must be kept in sync with linker script */
#define LOAD_OFFSET 0xffffffff80000000
#define PAGE_SHIFT 12

#define SATP_MODE (0x8000000000000000) /* Sv39 */

.section ".head.text","ax"
.global _start
.align 4
_start:
    /*
     * Image header expected by Linux boot-loaders
     */
    /* Jump to kernel start */
    j _start_kernel
    /* Reserved */
    .word 0
    .balign 8
    /* Image load offset from RAM start */
    .dword 0x00200000
    /* Kernel image size */
    .dword _end - _start
    /* Kernel flags */
    .dword 0
    /* Header version */
    .word 0x2
    /* Reserved */
    .word 0
    /* Reserved */
    .dword 0
    /* Magic number (deprecated) */
    .ascii "RISCV\0\0\0"
    .balign 4
    /* Magic number (deprecated) */
    .ascii "RSC\x05"
    .word 0

.align 2
relocate:
    /*
     * $a0 = kernel root page table
     */

    /* Relocate return address */
    li a2, LOAD_OFFSET
    la a3, _start
    sub a2, a2, a3
    add ra, ra, a2

    /* Point stvec to virtual address of intruction after satp write */
    la a3, 1f
    add a3, a3, a2
    csrw stvec, a3

    /*
     * Load kernel page directory, which will cause us to trap to stvec if VA != PA,
     * or simply fall through if VA == PA.  We need a full fence here because setup_early_vm()
     * just wrote these PTEs and we need to ensure the new translations are in use.
     */
    srl a3, a0, PAGE_SHIFT
    li a1, SATP_MODE
    or a3, a3, a1
    sfence.vma
    csrw satp, a3
.align 2
1:
    /* Set trap vector to spin forever to help debug */
    la a1, .Lsecondary_park
    csrw stvec, a1

    /* Reload the global pointer */
.option push
.option norelax
    la gp, __global_pointer$
.option pop

    /* Relocate the kernel root page table address and return it */
    add a0, a0, a2

    ret

.align 2
setup_trap_vector:
    /* Set trap vector to exception handler */
    la a0, trap_entry
    csrw stvec, a0
    csrw sscratch, zero
    ret

.Lsecondary_park:
    /* We lack SMP support or have too many harts, so park this hart */
    j .Lsecondary_park

.align 4
_start_kernel:
    /* Mask all interrupts */
    csrw sie, zero
    csrw sip, zero

    /* If hart ID is not #0, park it */
    bnez a0, .Lsecondary_park

    /* Save DTB pointer, will be later passed to arch_init() */
    mv s1, a1

.option push
.option norelax
    la gp, __global_pointer$
.option pop

    /* Clear BSS section */
    la a3, _sbss
    la a4, _ebss
    ble a4, a3, clear_bss_done
clear_bss:
    sd zero, (a3)
    add a3, a3, 8
    blt a3, a4, clear_bss
clear_bss_done:

    /* Allocate early stack pointer and setup dummy stack frame */
    la sp, _sstack
    addi sp, sp, -16
    sd zero, 8(sp)
    sd zero, 0(sp)
    addi s0, sp, 16

    /* Relocate to virtual address and save VA of root page table */
    call setup_early_vm
    call relocate
    mv s2, a0

    call setup_trap_vector

    /* Restore HLL environment */
    mv tp, zero
    la sp, _sstack

    /* Jump to Rust code */
    mv a0, s1
    mv a1, s2
    call arch_init
    tail kmain
