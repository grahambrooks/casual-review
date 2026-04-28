CARGO ?= cargo
BIN := target/release/cr
CURRENT := $(shell awk -F'"' '/^version = / {print $$2; exit}' Cargo.toml)

.DEFAULT_GOAL := help

.PHONY: help
help: ## Show this help
	@awk 'BEGIN {FS = ":.*?## "} \
	     /^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}' \
	     $(MAKEFILE_LIST) | sort
	@echo
	@echo "Current version: $(CURRENT)"

# ---------------------------------------------------------------------------
# Build / test
# ---------------------------------------------------------------------------

.PHONY: build
build: ## Compile in dev mode
	$(CARGO) build

.PHONY: dist
dist: $(BIN) ## Build the release binary at target/release/cr

$(BIN): $(shell find src -name '*.rs') Cargo.toml
	$(CARGO) build --release

.PHONY: check
check: ## Fast type-check without producing binaries
	$(CARGO) check --all-targets

.PHONY: test
test: ## Run all tests
	$(CARGO) test

.PHONY: bench
bench: ## Run criterion benchmarks (LOC/sec)
	$(CARGO) bench --bench throughput

.PHONY: clean
clean: ## Remove build artifacts
	$(CARGO) clean

# ---------------------------------------------------------------------------
# Lint / format
# ---------------------------------------------------------------------------

.PHONY: fmt
fmt: ## Format the codebase
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Check formatting without writing changes
	$(CARGO) fmt --all -- --check

.PHONY: clippy
clippy: ## Run clippy with -D warnings
	$(CARGO) clippy --all-targets -- -D warnings

.PHONY: lint
lint: fmt-check clippy ## fmt-check + clippy

# ---------------------------------------------------------------------------
# Snapshot tests
# ---------------------------------------------------------------------------

.PHONY: snapshots-review
snapshots-review: ## Interactively review pending insta snapshots
	$(CARGO) insta review

.PHONY: snapshots-accept
snapshots-accept: ## Accept all pending insta snapshots (use with care)
	$(CARGO) insta accept

# ---------------------------------------------------------------------------
# Self-evaluation
# ---------------------------------------------------------------------------

.PHONY: selfcheck
selfcheck: dist ## Run cr against the project's own src/ tree
	-./$(BIN) check --repo src --verbose

.PHONY: eval
eval: dist ## Run cr against the fixtures/ corpus (ignores exit code — fixtures intentionally trigger error-severity rules)
	-./$(BIN) check --repo fixtures --verbose

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------

.PHONY: install
install: ## cargo install --path . (drops `cr` in ~/.cargo/bin)
	$(CARGO) install --path . --locked

# ---------------------------------------------------------------------------
# Versioning / release
# ---------------------------------------------------------------------------

.PHONY: version
version: ## Print the current Cargo.toml version
	@echo $(CURRENT)

# Verify pre-release state: working tree clean, tests pass, lint clean.
.PHONY: verify-release
verify-release: lint test ## Pre-release verification (lint + test)
	@if [ -n "$$(git status --porcelain)" ]; then \
		echo "error: working tree is dirty"; \
		git status -s; \
		exit 1; \
	fi

# `make release VERSION=2026.5.0`
#
# Flow:
#   1. Validate VERSION format and that working tree is clean.
#   2. Run lint + test against the current source.
#   3. Rewrite Cargo.toml's version line.
#   4. Re-run cargo build to refresh Cargo.lock.
#   5. Re-run tests against the bumped version.
#   6. Commit Cargo.toml and create the v$(VERSION) tag locally.
#   7. Print the next step (push) — does NOT push automatically.
.PHONY: release
release: ## Bump version, run checks, commit + tag locally. Usage: make release VERSION=YYYY.M.PATCH
	@if [ -z "$(VERSION)" ]; then \
		echo "usage: make release VERSION=YYYY.M.PATCH"; \
		echo "current: $(CURRENT)"; \
		exit 1; \
	fi
	@if ! echo "$(VERSION)" | grep -qE '^[0-9]{4}\.[0-9]+\.[0-9]+$$'; then \
		echo "error: VERSION must match YYYY.M.PATCH (got: $(VERSION))"; \
		exit 1; \
	fi
	@if [ "$(VERSION)" = "$(CURRENT)" ]; then \
		echo "error: VERSION ($(VERSION)) matches current Cargo.toml version"; \
		exit 1; \
	fi
	@if [ -n "$$(git status --porcelain)" ]; then \
		echo "error: working tree is dirty; commit or stash first"; \
		git status -s; \
		exit 1; \
	fi
	@if git rev-parse --verify --quiet "v$(VERSION)" >/dev/null; then \
		echo "error: tag v$(VERSION) already exists"; \
		exit 1; \
	fi
	@echo "==> pre-flight: lint + test"
	@$(MAKE) --no-print-directory lint
	@$(MAKE) --no-print-directory test
	@echo "==> bumping Cargo.toml: $(CURRENT) -> $(VERSION)"
	@python3 -c "import re; \
		c = open('Cargo.toml').read(); \
		c = re.sub(r'^version = \"[^\"]+\"', 'version = \"$(VERSION)\"', c, count=1, flags=re.M); \
		open('Cargo.toml','w').write(c)"
	@echo "==> verifying bumped build"
	@$(CARGO) build --release --quiet
	@$(CARGO) test --quiet
	@echo "==> committing and tagging"
	@git add Cargo.toml
	@git commit -m "Release v$(VERSION)"
	@git tag -a "v$(VERSION)" -m "Release v$(VERSION)"
	@echo
	@echo "Tagged v$(VERSION) locally. Push to trigger the release workflow:"
	@echo "    git push origin main --follow-tags"
	@echo
	@echo "After push, the release workflow will:"
	@echo "    - build binaries for 5 platforms"
	@echo "    - upload to GitHub Releases"
	@echo "    - update Formula/casual-review.rb on main"
