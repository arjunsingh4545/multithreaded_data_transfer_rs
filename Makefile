# fastcp — every target is a thin alias for a script in scripts/.
# The scripts hold the logic (and work without make); run `make help` for the list.
#
# Pass options through ARGS, e.g.:
#   make build ARGS="--target x86_64-unknown-linux-musl"
#   make test  ARGS="unit -- engine::pipeline"
#   make install PREFIX=/usr/local      (run with sudo for system dirs)

SHELL       := bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help
S := scripts
ARGS ?=
export PREFIX

.PHONY: help build debug check quick test fmt fmt-check lint doc install uninstall completions clean

help: ## Show this list
	@awk 'BEGIN { FS = ":.*## " } /^[a-z-]+:.*## / { printf "  \033[36mmake %-12s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

build: ## Release build + smoke test
	@$(S)/build.sh $(ARGS)

debug: ## Debug build + smoke test
	@$(S)/build.sh --profile debug $(ARGS)

check: ## All quality gates
	@$(S)/check.sh $(ARGS)

quick: ## Pre-commit gates: fmt, layers, clippy, unit tests
	@$(S)/check.sh --quick $(ARGS)

test: ## Run tests; ARGS="unit|integration|cli|doc [-- filter]"
	@$(S)/test.sh $(ARGS)

fmt: ## Format the code
	@$(S)/fmt.sh $(ARGS)

fmt-check: ## Fail if code is not formatted
	@$(S)/fmt.sh --check

lint: ## Layer rules + clippy + shellcheck
	@$(S)/lint.sh $(ARGS)

doc: ## Build API docs; ARGS=--open to view
	@$(S)/doc.sh $(ARGS)

install: ## Install to ~/.local/bin (or $PREFIX/bin)
	@$(S)/install.sh $(ARGS)

uninstall: ## Remove the installed binary + completions
	@$(S)/uninstall.sh $(ARGS)

completions: ## Install completions for $SHELL
	@$(S)/completions.sh --install $(ARGS)

clean: ## Remove build output
	@$(S)/clean.sh $(ARGS)
