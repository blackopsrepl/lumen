# Playwright browser stack (architecture B)
SHELL := /usr/bin/env bash
BIN := ./bin
AGENT ?= default

.PHONY: help bootstrap install-host install-systemd install-skill build up down restart \
        status logs shell session dashboard feedback feedback-watch smoke test clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

bootstrap: ## New machine: install CLI+browser, build, up, services, skill, smoke
	$(BIN)/bootstrap.sh

install-host: ## Install @playwright/cli + its browser on the host
	$(BIN)/install-host.sh

install-systemd: ## Install + enable the user services (real paths baked in)
	$(BIN)/install-systemd.sh

install-skill: ## Install the opencode playwright-browser skill
	$(BIN)/install-skill.sh

build: ## Build the browser image
	$(BIN)/build.sh

up: ## Start the browser broker (detached)
	$(BIN)/up.sh

down: ## Stop the browser broker (keeps agent profiles)
	$(BIN)/down.sh

restart: ## Restart the browser broker
	$(BIN)/restart.sh

status: ## Broker status + registered agent browsers
	$(BIN)/status.sh

logs: ## Follow broker logs
	$(BIN)/logs.sh

shell: ## Shell inside the broker container
	$(BIN)/shell.sh

session: ## Ensure + attach an agent browser: make session AGENT=alice
	$(BIN)/session.sh $(AGENT)

pw: ## Run playwright-cli in the shared workspace: make pw ARGS="-s=alice snapshot"
	$(BIN)/pw.sh $(ARGS)

dashboard: ## Open the live dashboard (localhost)
	$(BIN)/dashboard.sh

feedback: ## Read a session's human-feedback inbox: make feedback AGENT=alice
	$(BIN)/feedback.sh $(AGENT)

feedback-watch: ## Run the feedback router in the foreground (service is preferred)
	$(BIN)/feedback-watch.sh

smoke: ## End-to-end smoke test
	$(BIN)/smoke-test.sh

test: smoke ## Alias for smoke

clean: ## Remove the image
	-podman rmi localhost/playwright-browser:$${PLAYWRIGHT_IMAGE_VERSION:-1.63.0}
