.section .rodata, "a"
.global ksyms_offsets
.weak ksyms_offsets
.balign 8
ksyms_offsets:
    .quad 0

.section .rodata, "a"
.global ksyms_num_syms
.weak ksyms_num_syms
.balign 8
ksyms_num_syms:
    .quad 0

.section .rodata, "a"
.global ksyms_markers
.weak ksyms_markers
.balign 8
ksyms_markers:
    .quad 0

.section .rodata, "a"
.global ksyms_names
.weak ksyms_names
.balign 8
ksyms_names:
    .quad 0
