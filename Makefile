# Lumen
SHELL := /usr/bin/env bash
BIN := ./bin
AGENT ?= default

.PHONY: help bootstrap install-host install-systemd install-skill build up down restart \
        status logs shell pw feedback smoke test clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

bootstrap: ## New machine: host CLI, image, service, skill, smoke
	$(BIN)/bootstrap.sh

install-host: ## Install @playwright/cli on the host
	$(BIN)/install-host.sh

install-systemd: ## Install + enable the lumen user service
	$(BIN)/install-systemd.sh

install-skill: ## Install the opencode lumen skill
	$(BIN)/install-skill.sh

build: ## Build the Lumen image
	$(BIN)/build.sh

up: ## Start the service (detached)
	$(BIN)/up.sh

down: ## Stop the service (keeps browser profiles)
	$(BIN)/down.sh

restart: ## Restart the service
	$(BIN)/restart.sh

status: ## Service state, health, and sessions
	$(BIN)/status.sh

logs: ## Follow service logs
	$(BIN)/logs.sh

shell: ## Shell inside the container
	$(BIN)/shell.sh

pw: ## Run playwright-cli: make pw ARGS="-s=alice snapshot"
	$(BIN)/pw.sh $(ARGS)

feedback: ## Read a session's feedback: make feedback AGENT=alice
	podman exec lumen lumen feedback $(AGENT) $(ARGS)

smoke: ## End-to-end smoke test
	$(BIN)/smoke.sh

test: smoke ## Alias for smoke

clean: ## Remove the image
	-podman rmi localhost/lumen:$${LUMEN_VERSION:-0.1.0}
