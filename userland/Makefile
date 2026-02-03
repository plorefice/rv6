# userland/Makefile

CROSS_COMPILE ?= riscv64-elf-

CC      := $(CROSS_COMPILE)gcc
AS      := $(CROSS_COMPILE)as
AR      := $(CROSS_COMPILE)ar
RANLIB  := $(CROSS_COMPILE)ranlib

USERLAND_DIR := $(CURDIR)
APPS_DIR     := $(USERLAND_DIR)/apps
LIB_DIR      := $(USERLAND_DIR)/lib
INCLUDE_DIR  := $(USERLAND_DIR)/include
BUILD_DIR    := $(USERLAND_DIR)/build

LIB_BUILD    := $(BUILD_DIR)/lib
CRT0_SRC     := $(LIB_DIR)/crt0.S
CRT0_OBJ     := $(LIB_BUILD)/crt0.o
LIB_A        := $(LIB_BUILD)/libuser.a

# Compile flags for the common library (tune as needed)
ASFLAGS ?=
CFLAGS  ?= -ffreestanding -fno-pic -fno-stack-protector -O2 -Wall -Wextra -nostdinc
CPPFLAGS?= -I$(INCLUDE_DIR)

# Discover all lib sources except crt0.S (crt0 linked separately)
LIB_SRCS_S := $(filter-out $(CRT0_SRC),$(wildcard $(LIB_DIR)/**/*.S) $(wildcard $(LIB_DIR)/*.S))
LIB_SRCS_C := $(wildcard $(LIB_DIR)/**/*.c) $(wildcard $(LIB_DIR)/*.c)
LIB_SRCS   := $(LIB_SRCS_S) $(LIB_SRCS_C)

# Map lib sources to objects under build/lib/, preserving subpaths
LIB_OBJS := $(patsubst $(LIB_DIR)/%, $(LIB_BUILD)/%, $(LIB_SRCS))
LIB_OBJS := $(LIB_OBJS:.S=.o)
LIB_OBJS := $(LIB_OBJS:.c=.o)

# Apps = any apps/*/Makefile
APP_MAKEFILES := $(wildcard $(APPS_DIR)/*/Makefile)
APPS          := $(notdir $(patsubst %/Makefile,%,$(APP_MAKEFILES)))

.PHONY: all list lib install clean distclean $(APPS)

all: lib $(APPS)

list:
	@echo "Apps: $(APPS)"

# ---- Common library ----

lib: $(CRT0_OBJ) $(LIB_A)

$(LIB_BUILD):
	@mkdir -p $@

# Ensure subdirs exist for nested sources
$(LIB_BUILD)/%/:
	@mkdir -p $@

# Assemble .S in lib/
$(LIB_BUILD)/%.o: $(LIB_DIR)/%.S
	@mkdir -p $(dir $@)
	$(CC) $(CPPFLAGS) $(ASFLAGS) -c $< -o $@

# Compile .c in lib/
$(LIB_BUILD)/%.o: $(LIB_DIR)/%.c
	@mkdir -p $(dir $@)
	$(CC) $(CPPFLAGS) $(CFLAGS) -c $< -o $@

# crt0 compiled separately (must not be archived; link it first)
$(CRT0_OBJ): $(CRT0_SRC) | $(LIB_BUILD)
	$(AS) $(ASFLAGS) $< -o $@

$(LIB_A): $(LIB_OBJS) | $(LIB_BUILD)
	@rm -f $@
	$(AR) rcs $@ $(LIB_OBJS)
	$(RANLIB) $@

# ---- Apps ----

$(APPS): lib
	$(MAKE) -C $(APPS_DIR)/$@

install: lib
	@for a in $(APPS); do \
		$(MAKE) -C $(APPS_DIR)/$$a install; \
	done

clean:
	@for a in $(APPS); do \
		$(MAKE) -C $(APPS_DIR)/$$a clean; \
	done

distclean: clean
	@rm -rf $(BUILD_DIR)
