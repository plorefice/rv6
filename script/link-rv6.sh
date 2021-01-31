#!/bin/sh

# This script uses the static libraries produced as part of the kernel's compilation steps and
# links them into the final ELF binary.
#
# Additionally, a symbol table is generated and linked in order to support symbol resolution in
# stack traces.
#
# Inspired by link-vmlinux.sh.

# Error out on error
set -eu

LD="${CROSS_COMPILE}"ld
CC="${CROSS_COMPILE}"gcc
RV6_LIBS="$@"

# Link of rv6
# ${1} - output file
# ${2}, ${3}, ... - optional extra .o files
link()
{
	local lds="linkers/riscv.ld"
	local output=${1}
	local objects
	local strip_debug

	# skip output file argument
	shift

	# The ksyms linking does not need debug symbols included.
	if [ "$output" != "${output#.tmp_rv6.ksyms}" ] ; then
		strip_debug=-Wl,--strip-debug
	fi

        objects="--whole-archive ${RV6_LIBS} --no-whole-archive ${@}"

        ${LD} ${strip_debug#-Wl,} -o ${output} -T ${lds} ${objects}
}

# Create ${2} .S file with all symbols from the ${1} object file
ksyms()
{
	nm ${1} | rg " [Tt] [a-zA-Z_]" | target/debug/ksymsgen > ${2}
}

# Perform one step in ksyms generation, including temporary linking of rv6.
ksyms_step()
{
	ksymso_prev=${ksymso}
	ksyms_rv6=.tmp_rv6.ksyms${1}
	ksymso=${ksyms_rv6}.o
	ksyms_S=${ksyms_rv6}.S

	link ${ksyms_rv6} "${ksymso_prev}"
	ksyms ${ksyms_rv6} ${ksyms_S}

	${CC} -march=rv64gc -mabi=lp64 -c -o ${ksymso} ${ksyms_S}
}

# Delete output files in case of error
cleanup()
{
	rm -f .tmp_rv6*
	rm -f rv6
	rm -f rv6.o
}

on_exit()
{
	if [ $? -ne 0 ]; then
		cleanup
	fi
}
trap on_exit EXIT

on_signals()
{
	exit 1
}
trap on_signals HUP INT QUIT TERM

if [ "$1" = "clean" ]; then
	cleanup
	exit 0
fi

ksymso=""
ksymso_prev=""
ksyms_rv6=""

# ksyms support
# Generate section listing all symbols and add it into rv6
# It's a three step process:
# 1)  Link .tmp_rv6.ksyms1 so it has all symbols and sections, but the ksyms section is empty.
#     Running ksyms on that gives us .tmp_ksyms1.o with the right size.
# 2)  Link .tmp_rv6.ksyms2 so it now has a ksyms section of the right size, but due to the added
#     section, some addresses have shifted. From here, we generate a correct .tmp_ksyms2.o.
# 3)  That link may have expanded the kernel image enough that more linker branch stubs/trampolines
#     had to be added, which introduces new names, which further expands ksyms. Do another pass
#     just in the case. In theory it's possible this results in even more stubs, but unlikely.
# 4)  The correct ${ksymso} is linked into the final rv6.

ksyms_step 1
ksyms_step 2
ksyms_step 3
link rv6 "${ksymso}"

rm .tmp_rv6.ksyms*
