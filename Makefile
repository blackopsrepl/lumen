# Lumen Makefile — one service, one command per job.

# ============== Colors & Symbols ==============
GREEN := \033[92m
CYAN := \033[96m
YELLOW := \033[93m
RED := \033[91m
GRAY := \033[90m
BOLD := \033[1m
RESET := \033[0m

CHECK := ✓
CROSS := ✗
ARROW := ▸
PROGRESS := →

# ============== Project Metadata ==============
VERSION := $(shell grep -m1 '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
BIN := ./bin
AGENT ?= default
ARGS ?=
LUMEN_TEST_PORT ?= 18899
# Container access goes through bin/ctr.sh so make targets share bin/common.sh's
# resolution (runtime auto-detect plus docker-via-sudo when the daemon needs it).

.DEFAULT_GOAL := help

.PHONY: help version banner bootstrap install-host install-systemd install-skill \
        build up down restart status logs shell pw feedback \
        fmt fmt-check clippy lint test-unit smoke ui-test test ci clean

# ============== Banner & Meta ==============

banner:
	@printf "$(CYAN)$(BOLD)◆ Lumen$(RESET) $(GRAY)v$(VERSION)$(RESET) $(GRAY)— sessions for agents, eyes for humans$(RESET)\n\n"

version: banner
	@printf "$(CYAN)Service version:$(RESET) $(YELLOW)$(BOLD)v$(VERSION)$(RESET)\n"
	@printf "$(CYAN)Service port:$(RESET)    $(YELLOW)$(BOLD)$${LUMEN_PORT:-8899}$(RESET)$(GRAY) (override via LUMEN_PORT in .env)$(RESET)\n"
	@printf "$(CYAN)E2E test port:$(RESET)   $(YELLOW)$(BOLD)$(LUMEN_TEST_PORT)$(RESET)$(GRAY) (override via LUMEN_TEST_PORT)$(RESET)\n\n"

# ============== Setup ==============

bootstrap: ## New machine: host CLI, image, service, skill, smoke
	@printf "$(ARROW) $(BOLD)Bootstrap: host CLI, image, service, skill, smoke$(RESET)\n"
	@$(BIN)/bootstrap.sh && printf "\n$(GREEN)$(BOLD)$(CHECK) Bootstrap complete — http://127.0.0.1:$${LUMEN_PORT:-8899}/$(RESET)\n\n" \
		|| (printf "\n$(RED)$(CROSS) Bootstrap failed$(RESET)\n\n" && exit 1)

install-host: ## Install @playwright/cli on the host
	@printf "$(ARROW) Installing host CLI...\n"
	@$(BIN)/install-host.sh && printf "$(GREEN)$(CHECK) Host CLI installed$(RESET)\n\n"

install-systemd: ## Install + enable the lumen user service
	@printf "$(ARROW) Installing systemd user unit...\n"
	@$(BIN)/install-systemd.sh && printf "$(GREEN)$(CHECK) Service unit enabled$(RESET)\n\n"

install-skill: ## Install the opencode lumen skill
	@printf "$(ARROW) Installing opencode skill...\n"
	@$(BIN)/install-skill.sh && printf "$(GREEN)$(CHECK) Skill installed$(RESET)\n\n"

# ============== Service ==============

build: ## Build the container image
	@printf "$(ARROW) $(BOLD)Building container image...$(RESET)\n"
	@$(BIN)/build.sh && printf "$(GREEN)$(CHECK) Image built$(RESET)\n\n"

up: ## Start the service (detached)
	@printf "$(ARROW) Starting service...\n"
	@$(BIN)/up.sh && printf "$(GREEN)$(CHECK) Lumen: http://127.0.0.1:$${LUMEN_PORT:-8899}/$(RESET)\n\n"

down: ## Stop the service (session profiles are ephemeral; /data is kept)
	@printf "$(ARROW) Stopping service...\n"
	@$(BIN)/down.sh && printf "$(GREEN)$(CHECK) Stopped$(RESET)\n\n"

restart: ## Restart the service
	@printf "$(ARROW) Restarting service...\n"
	@$(BIN)/restart.sh && printf "$(GREEN)$(CHECK) Restarted: http://127.0.0.1:$${LUMEN_PORT:-8899}/$(RESET)\n\n"

status: ## Service state, health, and sessions
	@$(BIN)/status.sh

logs: ## Follow service logs
	@$(BIN)/logs.sh

shell: ## Shell inside the container
	@$(BIN)/shell.sh

# ============== Agent CLI ==============

pw: ## Run playwright-cli: make pw ARGS="-s=alice snapshot"
	@$(BIN)/pw.sh $(ARGS)

feedback: ## Read a session's feedback: make feedback AGENT=alice
	@$(BIN)/ctr.sh exec lumen lumen feedback $(AGENT) $(ARGS)

# ============== Quality Gates ==============

fmt: ## Format all Rust code
	@printf "$(PROGRESS) Formatting code...\n"
	@cargo fmt --all
	@printf "$(GREEN)$(CHECK) Code formatted$(RESET)\n\n"

fmt-check: ## Verify formatting (CI gate 1)
	@printf "$(PROGRESS) Checking formatting...\n"
	@cargo fmt --all -- --check \
		&& printf "$(GREEN)$(CHECK) Formatting valid$(RESET)\n" \
		|| (printf "$(RED)$(CROSS) Formatting issues — run make fmt$(RESET)\n" && exit 1)

clippy: ## Run clippy with CI strictness (CI gate 2)
	@printf "$(PROGRESS) Running clippy...\n"
	@cargo clippy --all-targets --locked -- -D warnings \
		&& printf "$(GREEN)$(CHECK) Clippy clean$(RESET)\n" \
		|| (printf "$(RED)$(CROSS) Clippy warnings — fix before committing$(RESET)\n" && exit 1)

lint: fmt-check clippy ## fmt-check + clippy
	@printf "\n$(GREEN)$(BOLD)$(CHECK) All lint checks passed$(RESET)\n\n"

test-unit: ## Rust unit tests (CI gate 3)
	@printf "$(PROGRESS) Running Rust tests...\n"
	@cargo test --locked \
		&& printf "\n$(GREEN)$(CHECK) Rust tests passed$(RESET)\n\n" \
		|| (printf "\n$(RED)$(CROSS) Rust tests failed$(RESET)\n\n" && exit 1)

smoke: ## Black-box smoke test against the live service
	@printf "$(ARROW) $(BOLD)Smoke test vs live service on port $${LUMEN_PORT:-8899}$(RESET)\n"
	@$(BIN)/smoke.sh \
		&& printf "$(GREEN)$(CHECK) Smoke passed$(RESET)\n\n" \
		|| (printf "$(RED)$(CROSS) Smoke failed$(RESET)\n\n" && exit 1)

ui-test: ## Browser-driven E2E on a disposable service (CI gate 5)
	@npm ci --silent
	@npx playwright install chromium >/dev/null
	@$(BIN)/ui-test.sh \
		&& printf "$(GREEN)$(CHECK) Viewer E2E passed$(RESET)\n\n" \
		|| (printf "$(RED)$(CROSS) Viewer E2E failed$(RESET)\n\n" && exit 1)

test: smoke ui-test ## Everything: smoke + browser E2E
	@printf "$(GREEN)$(BOLD)$(CHECK) Full test suite passed$(RESET)\n\n"

ci: ## Local mirror of the Forgejo CI gates, in CI order
	@printf "$(CYAN)$(BOLD)╔═══════════════════════════════════════════════════╗$(RESET)\n"
	@printf "$(CYAN)$(BOLD)║      Local CI — mirror of Forgejo gates           ║$(RESET)\n"
	@printf "$(CYAN)$(BOLD)╚═══════════════════════════════════════════════════╝$(RESET)\n\n"
	@printf "$(PROGRESS) Step 1/5: Formatting...\n"
	@$(MAKE) --no-print-directory fmt-check
	@printf "$(PROGRESS) Step 2/5: Clippy...\n"
	@$(MAKE) --no-print-directory clippy
	@printf "$(PROGRESS) Step 3/5: Rust tests...\n"
	@cargo test --locked --quiet \
		&& printf "$(GREEN)$(CHECK) Rust tests passed$(RESET)\n" \
		|| (printf "$(RED)$(CROSS) Rust tests failed$(RESET)\n" && exit 1)
	@printf "$(PROGRESS) Step 4/5: Release build...\n"
	@cargo build --release --locked --quiet \
		&& printf "$(GREEN)$(CHECK) Release build passed$(RESET)\n" \
		|| (printf "$(RED)$(CROSS) Release build failed$(RESET)\n" && exit 1)
	@printf "$(PROGRESS) Step 5/5: Viewer E2E...\n"
	@$(MAKE) --no-print-directory ui-test
	@printf "$(GREEN)$(BOLD)╔═══════════════════════════════════════════════════╗$(RESET)\n"
	@printf "$(GREEN)$(BOLD)║      ✓ CI GATES PASSED                              ║$(RESET)\n"
	@printf "$(GREEN)$(BOLD)╚═══════════════════════════════════════════════════╝$(RESET)\n\n"

# ============== Cleanup ==============

clean: ## Remove the container image
	@printf "$(ARROW) Removing container image...\n"
	@-$(BIN)/ctr.sh rmi localhost/lumen:$${LUMEN_VERSION:-0.11.6}
	@printf "$(GREEN)$(CHECK) Clean complete$(RESET)\n\n"

# ============== Help ==============

help: banner
	@printf "$(CYAN)$(BOLD)Setup:$(RESET)\n"
	@printf "  $(GREEN)make bootstrap$(RESET)        One-command setup for a new machine\n"
	@printf "  $(GREEN)make install-host$(RESET)     Install the agent CLI on this host\n"
	@printf "  $(GREEN)make install-systemd$(RESET)  Keep the service running (systemd user unit)\n"
	@printf "  $(GREEN)make install-skill$(RESET)    Install the opencode skill for agents\n"
	@printf "\n"
	@printf "$(CYAN)$(BOLD)Service:$(RESET)\n"
	@printf "  $(GREEN)make build$(RESET)            Build the container image\n"
	@printf "  $(GREEN)make up$(RESET)               Start the service\n"
	@printf "  $(GREEN)make down$(RESET)             Stop the service (session profiles are ephemeral; /data is kept)\n"
	@printf "  $(GREEN)make restart$(RESET)          Restart the service\n"
	@printf "  $(GREEN)make status$(RESET)           Container state, health, sessions\n"
	@printf "  $(GREEN)make logs$(RESET)             Follow service logs\n"
	@printf "  $(GREEN)make shell$(RESET)            Shell inside the container\n"
	@printf "\n"
	@printf "$(CYAN)$(BOLD)Agent CLI:$(RESET)\n"
	@printf "  $(GREEN)make pw$(RESET) ARGS=\"-s=alice snapshot\"   Drive a session's browser\n"
	@printf "  $(GREEN)make feedback$(RESET) AGENT=alice          Read a session's human notes\n"
	@printf "\n"
	@printf "$(CYAN)$(BOLD)Quality:$(RESET)\n"
	@printf "  $(GREEN)make ci$(RESET)               $(YELLOW)$(BOLD)All CI gates, in CI order$(RESET)\n"
	@printf "  $(GREEN)make lint$(RESET)             Formatting + clippy\n"
	@printf "  $(GREEN)make fmt$(RESET)              Format all Rust code\n"
	@printf "  $(GREEN)make test-unit$(RESET)        Rust unit tests\n"
	@printf "  $(GREEN)make smoke$(RESET)            Smoke test vs the live service\n"
	@printf "  $(GREEN)make ui-test$(RESET)          Browser E2E on a disposable service\n"
	@printf "  $(GREEN)make test$(RESET)             smoke + ui-test\n"
	@printf "\n"
	@printf "$(CYAN)$(BOLD)Other:$(RESET)\n"
	@printf "  $(GREEN)make version$(RESET)          Show version and ports\n"
	@printf "  $(GREEN)make clean$(RESET)            Remove the container image\n"
	@printf "  $(GREEN)make help$(RESET)             This help\n"
	@printf "\n"
	@printf "$(GRAY)Service http://127.0.0.1:$${LUMEN_PORT:-8899} · E2E port $(LUMEN_TEST_PORT) · single test: cargo test <name>$(RESET)\n\n"
