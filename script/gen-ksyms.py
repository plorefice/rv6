#!/usr/bin/env python3

import sys
import itertools

fns = sorted([line.strip().split(" ") for line in sys.stdin])
ksyms_num = len(fns)
ksyms_names = [fn[2] for fn in fns]
ksyms_offsets = [fn[0].lstrip('0') for fn in fns]
ksyms_markers = list(itertools.accumulate(
    [len(name) for name in ksyms_names], lambda x, y: x+y+1, initial=0))

print('.section .rodata, "a"')
print('.global ksyms_offsets')
print('.balign 8')
print('ksyms_offsets:')
for off in ksyms_offsets:
    print("    .quad 0x" + off)
print()
print('.global ksyms_num_syms')
print('.balign 8')
print('ksyms_num_syms:')
print('    .quad', int(ksyms_num))
print()
print('.global ksyms_markers')
print('.balign 8')
print('ksyms_markers:')
for marker in ksyms_markers:
    print('    .quad', int(marker))
print('.global ksyms_names')
print('.balign 8')
print('ksyms_names:')
for name in ksyms_names:
    print('    .asciz "' + name + '"')
print()
