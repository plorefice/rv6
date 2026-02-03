# userland/mk/app.mk
#
# Common build template for a single app.
# Per-app Makefile must set:
#   APP := <name>
#
# Optional:
#   SRCS := entry.S other.S foo.c ...
#   LDSCRIPT := <path>             (default: userland/linker/user.ld)
#   CROSS_COMPILE := riscv64-elf-
#
# Install-related:
#   DESTDIR := <rootfs staging dir>    (default: userland/../out/rootfs)
#   PREFIX  := /                       (default: /)
#   BINDIR  := bin                     (default: bin)
#   INSTALL_SUBDIR := <dir>            (default: $(BINDIR))
#   INSTALL_NAME   := <filename>       (default: $(APP))
#
# Examples:
#   init: INSTALL_SUBDIR := .
#         INSTALL_NAME   := init
#   normal app: INSTALL_SUBDIR := bin

CROSS_COMPILE ?= riscv64-elf-

AS      := $(CROSS_COMPILE)as
CC      := $(CROSS_COMPILE)gcc
LD      := $(CROSS_COMPILE)ld
OBJCOPY := $(CROSS_COMPILE)objcopy

# Root of userland tree and output location
USERLAND_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_ROOT   ?= $(USERLAND_DIR)/build
OUTDIR       := $(BUILD_ROOT)/$(APP)

# Default sources and linker script
SRCS     ?= entry.S
LDSCRIPT ?= $(USERLAND_DIR)/linker/user.ld

# Basic flags
ASFLAGS ?=
CFLAGS  ?= -ffreestanding -fno-pic -fno-stack-protector -O2 -Wall -Wextra
LDFLAGS ?=

# Install defaults
# Stage into repo-root out/rootfs by default (matches your existing convention)
DESTDIR         ?= $(abspath $(USERLAND_DIR)/../out/rootfs)
PREFIX          ?= /
BINDIR          ?= bin
INSTALL_SUBDIR  ?= $(BINDIR)
INSTALL_NAME    ?= $(APP)

# Tools
INSTALL ?= install

# Derived lists
OBJS := $(addprefix $(OUTDIR)/,$(addsuffix .o,$(basename $(SRCS))))

.PHONY: all clean dirs install
all: $(OUTDIR)/$(APP)

dirs:
	@mkdir -p $(OUTDIR)

# Assemble .S
$(OUTDIR)/%.o: %.S | dirs
	$(AS) $(ASFLAGS) $< -o $@

# Compile .c
$(OUTDIR)/%.o: %.c | dirs
	$(CC) $(CFLAGS) -c $< -o $@

# Link ELF
$(OUTDIR)/$(APP).elf: $(OBJS) $(LDSCRIPT) | dirs
	$(LD) -T $(LDSCRIPT) $(LDFLAGS) -o $@ $(OBJS)

# Flat binary
$(OUTDIR)/$(APP): $(OUTDIR)/$(APP).elf | dirs
	$(OBJCOPY) -O binary $< $@

# Install into rootfs staging area
install: $(OUTDIR)/$(APP)
	@dest="$(DESTDIR)$(PREFIX)$(INSTALL_SUBDIR)"; \
	mkdir -p "$$dest"; \
	$(INSTALL) -m 0755 "$(OUTDIR)/$(APP)" "$$dest/$(INSTALL_NAME)"

clean:
	@rm -rf $(OUTDIR)
