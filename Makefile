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

# Two release flows:
#
#   make release                          — tag whatever's in Cargo.toml right now.
#                                           Assumes the version is committed and
#                                           ready to ship. Useful when you've
#                                           already bumped, or for the first
#                                           release of an existing version.
#
#   make release VERSION=2026.5.0         — bump Cargo.toml to VERSION, commit,
#                                           and tag in one step.
#
# Both gate on a clean working tree, an unused tag, and passing lint + test.
# Neither pushes — the next-step push is printed.
.PHONY: release
release: ## Tag current version, or bump+tag with VERSION=YYYY.M.PATCH
	@target_version="$(VERSION)"; \
	if [ -z "$$target_version" ]; then \
		target_version="$(CURRENT)"; \
		mode="tag-only"; \
	else \
		mode="bump-and-tag"; \
		if ! echo "$$target_version" | grep -qE '^[0-9]{4}\.[0-9]+\.[0-9]+$$'; then \
			echo "error: VERSION must match YYYY.M.PATCH (got: $$target_version)"; \
			exit 1; \
		fi; \
		if [ "$$target_version" = "$(CURRENT)" ]; then \
			echo "error: VERSION ($$target_version) matches current Cargo.toml."; \
			echo "       Drop VERSION=... to tag the current version instead."; \
			exit 1; \
		fi; \
	fi; \
	if [ -n "$$(git status --porcelain)" ]; then \
		echo "error: working tree is dirty; commit or stash first"; \
		git status -s; \
		exit 1; \
	fi; \
	if git rev-parse --verify --quiet "v$$target_version" >/dev/null; then \
		echo "error: tag v$$target_version already exists"; \
		exit 1; \
	fi; \
	echo "==> mode: $$mode (target: v$$target_version)"; \
	echo "==> pre-flight: lint + test"; \
	$(MAKE) --no-print-directory lint; \
	$(MAKE) --no-print-directory test; \
	if [ "$$mode" = "bump-and-tag" ]; then \
		echo "==> bumping Cargo.toml: $(CURRENT) -> $$target_version"; \
		python3 -c "import re; \
			c = open('Cargo.toml').read(); \
			c = re.sub(r'^version = \"[^\"]+\"', 'version = \"'\"$$target_version\"'\"', c, count=1, flags=re.M); \
			open('Cargo.toml','w').write(c)"; \
		echo "==> verifying bumped build"; \
		$(CARGO) build --release --quiet; \
		$(CARGO) test --quiet; \
		echo "==> committing"; \
		git add Cargo.toml; \
		git commit -m "Release v$$target_version"; \
	fi; \
	echo "==> tagging v$$target_version"; \
	git tag -a "v$$target_version" -m "Release v$$target_version"; \
	echo; \
	echo "Tagged v$$target_version locally. Push to trigger the release workflow:"; \
	echo "    git push origin main --follow-tags"; \
	echo; \
	echo "After push, the release workflow will:"; \
	echo "    - build binaries for 5 platforms"; \
	echo "    - upload to GitHub Releases"; \
	echo "    - update Formula/casual-review.rb on main"
