# Lumen — build, test, install.
#
# Lumen is a native Qt application plus its daemon. There is no container, no
# HTTP surface and no browser: the daemon owns a Wayland compositor, every
# session is a nested Wayland client of it, and agents reach it over a unix
# socket in the user's runtime directory.

BUILD_DIR ?= build
CMAKE_FLAGS ?= -DCMAKE_BUILD_TYPE=Debug
PREFIX ?= $(HOME)/.local

# The daemon and the viewer are Qt applications: they need a display stack but
# never a visible window, and the tests must not touch the operator's session.
TEST_ENV = QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=

.DEFAULT_GOAL := help

.PHONY: help build test install service logs clean

help: ## List targets
	@grep -hE '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk -F':.*?## ' '{printf "  \033[1m%-10s\033[0m %s\n", $$1, $$2}'

build: ## Configure and build every target
	cmake -B $(BUILD_DIR) $(CMAKE_FLAGS)
	cmake --build $(BUILD_DIR) -j$$(nproc)

test: build ## Run the unit and integration suite
	cd $(BUILD_DIR) && $(TEST_ENV) ctest --output-on-failure

install: build ## Install the binaries and the daemon user service
	PREFIX=$(PREFIX) BUILD_DIR=$(BUILD_DIR) ./bin/install.sh

service: ## Show the daemon service state
	@systemctl --user status lumen.service --no-pager || true

logs: ## Follow the daemon's logs
	@journalctl --user -u lumen.service -f

clean: ## Remove the build tree
	rm -rf $(BUILD_DIR)
