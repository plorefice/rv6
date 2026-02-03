# userland/Makefile
#
# Build common userland library (libuser.a + crt0.o), then delegate to apps.
#
# Configure which lib sources to build by setting:
#   LIB_DIRS  += lib syscall string
#   LIB_FILES += extra/foo.c extra/bar.S
#
# Paths are relative to userland/lib/.

CROSS_COMPILE ?= riscv64-elf-

CC     := $(CROSS_COMPILE)gcc
AR     := $(CROSS_COMPILE)ar
RANLIB := $(CROSS_COMPILE)ranlib

USERLAND_DIR := $(CURDIR)
APPS_DIR     := $(USERLAND_DIR)/apps
LIB_ROOT     := $(USERLAND_DIR)/lib
INCLUDE_DIR  := $(USERLAND_DIR)/include
BUILD_DIR    := $(USERLAND_DIR)/build

LIB_BUILD := $(BUILD_DIR)/lib
CRT0_SRC  := $(LIB_ROOT)/crt0.S
CRT0_OBJ  := $(LIB_BUILD)/crt0.o
LIB_A     := $(LIB_BUILD)/libuser.a

CPPFLAGS ?= -I$(INCLUDE_DIR)
CFLAGS   ?= -ffreestanding -fno-pic -fno-stack-protector -O2 -Wall -Wextra -nostdinc
ASFLAGS  ?=

# ---- Configuration ----

# Non-recursive: list directories under userland/lib to scan for *.c and *.S
LIB_DIRS  ?=

# Individual files under userland/lib to include (relative paths)
LIB_FILES ?= syscall.S write.c

# ---- Derived lists ----

# Expand directory entries into file lists (NON-recursive)
LIB_SRCS_FROM_DIRS := $(foreach d,$(LIB_DIRS), \
	$(wildcard $(LIB_ROOT)/$(d)/*.c) $(wildcard $(LIB_ROOT)/$(d)/*.S))

LIB_SRCS_FROM_FILES := $(addprefix $(LIB_ROOT)/,$(LIB_FILES))

# Combine, drop crt0.S if someone listed it by mistake
LIB_SRCS := $(filter-out $(CRT0_SRC),$(LIB_SRCS_FROM_DIRS) $(LIB_SRCS_FROM_FILES))

# Map lib sources to objects under build/lib/, preserving relative path under lib/
LIB_OBJS := $(patsubst $(LIB_ROOT)/%,$(LIB_BUILD)/%,$(LIB_SRCS))
LIB_OBJS := $(LIB_OBJS:.c=.o)
LIB_OBJS := $(LIB_OBJS:.S=.o)

# Apps = any apps/*/Makefile
APP_MAKEFILES := $(wildcard $(APPS_DIR)/*/Makefile)
APPS          := $(notdir $(patsubst %/Makefile,%,$(APP_MAKEFILES)))

.PHONY: all list lib install clean distclean $(APPS)

all: lib $(APPS)

list:
	@echo "Apps: $(APPS)"
	@echo "LIB_DIRS:  $(LIB_DIRS)"
	@echo "LIB_FILES: $(LIB_FILES)"
	@echo "LIB_SRCS:  $(patsubst $(LIB_ROOT)/%,%,$(LIB_SRCS))"

# ---- Common library ----

lib: $(CRT0_OBJ) $(LIB_A)

$(LIB_BUILD):
	@mkdir -p "$@"

# Build preprocessed assembly (.S) with gcc so -I include paths work
$(LIB_BUILD)/%.o: $(LIB_ROOT)/%.S
	@mkdir -p "$(dir $@)"
	$(CC) $(CPPFLAGS) $(ASFLAGS) -c "$<" -o "$@"

$(LIB_BUILD)/%.o: $(LIB_ROOT)/%.c
	@mkdir -p "$(dir $@)"
	$(CC) $(CPPFLAGS) $(CFLAGS) -c "$<" -o "$@"

$(CRT0_OBJ): $(CRT0_SRC) | $(LIB_BUILD)
	$(CC) $(CPPFLAGS) $(ASFLAGS) -c "$<" -o "$@"

$(LIB_A): $(LIB_OBJS) | $(LIB_BUILD)
	@rm -f "$@"
	$(AR) rcs "$@" $(LIB_OBJS)
	$(RANLIB) "$@"

# ---- Apps ----

$(APPS): lib
	$(MAKE) -C "$(APPS_DIR)/$@"

install: lib
	@for a in $(APPS); do \
		$(MAKE) -C "$(APPS_DIR)/$$a" install; \
	done

clean:
	@for a in $(APPS); do \
		$(MAKE) -C "$(APPS_DIR)/$$a" clean; \
	done

distclean: clean
	@rm -rf "$(BUILD_DIR)"
