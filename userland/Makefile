# userland/Makefile
#
# Discovers apps under userland/apps/* that contain a Makefile and delegates.
# Build outputs: userland/build/<app>/...
# Install staging: DESTDIR (default provided by app.mk)

APPS_DIR   := $(CURDIR)/apps
BUILD_DIR  := $(CURDIR)/build

APP_MAKEFILES := $(wildcard $(APPS_DIR)/*/Makefile)
APPS          := $(notdir $(patsubst %/Makefile,%,$(APP_MAKEFILES)))

.PHONY: all list clean distclean install $(APPS)

all: $(APPS)

list:
	@echo "$(APPS)"

$(APPS):
	$(MAKE) -C $(APPS_DIR)/$@

install:
	@for a in $(APPS); do \
		$(MAKE) -C $(APPS_DIR)/$$a install; \
	done

# You can also do: make install DESTDIR=/path/to/rootfs
# It will flow through to the per-app makefiles automatically.

clean:
	@for a in $(APPS); do \
		$(MAKE) -C $(APPS_DIR)/$$a clean; \
	done

distclean:
	@rm -rf $(BUILD_DIR)
