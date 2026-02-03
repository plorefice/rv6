# userland/mk/app.mk
#
# Per-app Makefile must set:
#   APP := <name>
#
# Optional:
#   SRCS := entry.S foo.c bar.S ...
#   LDSCRIPT := <path> (default: userland/linker/user.ld)
#   CROSS_COMPILE := riscv64-elf-
#
# Install:
#   DESTDIR, PREFIX, INSTALL_SUBDIR, INSTALL_NAME (as before)

CROSS_COMPILE ?= riscv64-elf-

AS      := $(CROSS_COMPILE)as
CC      := $(CROSS_COMPILE)gcc
OBJCOPY := $(CROSS_COMPILE)objcopy

USERLAND_DIR ?= $(abspath $(CURDIR)/../..)
INCLUDE_DIR  ?= $(USERLAND_DIR)/include
BUILD_ROOT   ?= $(USERLAND_DIR)/build
OUTDIR       := $(BUILD_ROOT)/$(APP)

SRCS     ?= entry.S
LDSCRIPT ?= $(USERLAND_DIR)/linker/user.ld

# Common library artifacts built by userland/Makefile
CRT0_OBJ := $(BUILD_ROOT)/lib/crt0.o
LIB_A    := $(BUILD_ROOT)/lib/libuser.a

# Flags
ASFLAGS   ?=
CFLAGS    ?= -ffreestanding -fno-pic -fno-stack-protector -O2 -Wall -Wextra -nostdinc
CPPFLAGS  ?= -I$(INCLUDE_DIR)
# Link with gcc driver (better once you have C; still fine for pure asm)
# -nostdlib: no host libc
# -lgcc: pull compiler support routines as needed
LDFLAGS   ?= -nostdlib
LDLIBS    ?= -lgcc

# Install defaults
DESTDIR         ?= $(abspath $(USERLAND_DIR)/../out/rootfs)
PREFIX          ?= /
BINDIR          ?= bin
INSTALL_SUBDIR  ?= $(BINDIR)
INSTALL_NAME    ?= $(APP)
INSTALL         ?= install

OBJS := $(addprefix $(OUTDIR)/,$(addsuffix .o,$(basename $(SRCS))))

.PHONY: all clean dirs install

all: $(OUTDIR)/$(APP)

dirs:
	@mkdir -p $(OUTDIR)

# Assemble app .S
$(OUTDIR)/%.o: %.S | dirs
	$(CC) $(CPPFLAGS) $(ASFLAGS) -c $< -o $@

# Compile app .c
$(OUTDIR)/%.o: %.c | dirs
	$(CC) $(CPPFLAGS) $(CFLAGS) -c $< -o $@

# Ensure common lib is built (delegates to top-level userland Makefile)
$(CRT0_OBJ) $(LIB_A):
	$(MAKE) -C $(USERLAND_DIR) lib

# Link app ELF (crt0 first, then app objs, then lib)
$(OUTDIR)/$(APP).elf: $(OBJS) $(CRT0_OBJ) $(LIB_A) $(LDSCRIPT) | dirs
	$(CC) $(LDFLAGS) -Wl,-T,$(LDSCRIPT) -o $@ \
	  $(CRT0_OBJ) $(OBJS) $(LIB_A) $(LDLIBS)

# Flat binary
$(OUTDIR)/$(APP): $(OUTDIR)/$(APP).elf | dirs
	$(OBJCOPY) -O binary $< $@

install: $(OUTDIR)/$(APP)
	@dest="$(DESTDIR)$(PREFIX)$(INSTALL_SUBDIR)"; \
	mkdir -p "$$dest"; \
	$(INSTALL) -m 0755 "$(OUTDIR)/$(APP)" "$$dest/$(INSTALL_NAME)"

clean:
	@rm -rf $(OUTDIR)
