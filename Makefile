CARGO ?= cargo
BIN := target/release/cr
CURRENT := $(shell awk -F'"' '/^version = / {print $$2; exit}' Cargo.toml)

VSCODE_DIR := extensions/vscode
JETBRAINS_DIR := extensions/jetbrains
ZED_DIR := extensions/zed
ZED_WASM_TARGET := wasm32-wasip1
EXT_OUT := target/extensions
GRADLEW := ./gradlew --no-daemon

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
check: fmt-check lint test ## The pre-push gate: the same commands as the CI check job

.PHONY: test
test: ## Run all tests (the CI test line)
	$(CARGO) test --workspace --all-features --locked

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
	$(CARGO) fmt --all --check

.PHONY: lint
lint: ## Run clippy with -D warnings (the CI clippy line)
	$(CARGO) clippy --workspace --all-targets --all-features --locked -- -D warnings

.PHONY: clippy
clippy: lint ## Alias for lint

.PHONY: hooks
hooks: ## Install the git hooks (prek: pre-commit and pre-push)
	prek install

# ---------------------------------------------------------------------------
# Editor extensions
# ---------------------------------------------------------------------------

.PHONY: ext-vscode
ext-vscode: ## Compile the VS Code extension (extensions/vscode/out/)
	cd $(VSCODE_DIR) && npm install --no-audit --no-fund && npm run compile

.PHONY: ext-vscode-package
ext-vscode-package: ext-vscode ## Package the VS Code extension as a .vsix in target/extensions/
	@mkdir -p $(EXT_OUT)
	cd $(VSCODE_DIR) && npx --yes @vscode/vsce@latest package \
		--no-dependencies \
		--out ../../$(EXT_OUT)/

.PHONY: ext-jetbrains
ext-jetbrains: ## Build the JetBrains plugin .zip
	cd $(JETBRAINS_DIR) && $(GRADLEW) buildPlugin
	@mkdir -p $(EXT_OUT)
	@cp $(JETBRAINS_DIR)/build/distributions/*.zip $(EXT_OUT)/ 2>/dev/null || true

.PHONY: ext-zed
ext-zed: ## Build the Zed extension (wasm32-wasip1)
	rustup target add $(ZED_WASM_TARGET) >/dev/null 2>&1 || true
	cd $(ZED_DIR) && $(CARGO) build --release --target $(ZED_WASM_TARGET)
	@mkdir -p $(EXT_OUT)
	@cp $(ZED_DIR)/target/$(ZED_WASM_TARGET)/release/casual_review_zed.wasm $(EXT_OUT)/ 2>/dev/null || true

.PHONY: extensions
extensions: ext-vscode ext-jetbrains ext-zed ## Build all editor extensions

.PHONY: clean-extensions
clean-extensions: ## Remove extension build artifacts (out/, build/, node_modules/, zed target/)
	rm -rf $(VSCODE_DIR)/out $(VSCODE_DIR)/node_modules
	-cd $(JETBRAINS_DIR) && $(GRADLEW) clean
	rm -rf $(ZED_DIR)/target
	rm -rf $(EXT_OUT)

# ---------------------------------------------------------------------------
# Run editor extensions for user testing
# ---------------------------------------------------------------------------
# Each target launches an editor session with the in-tree extension loaded.
# The extensions shell out to `cr`, so make sure `cr` is on $$PATH first
# (e.g. via `make install`).
#
# EXT_RUN_DIR overrides the folder the editor opens. Defaults to the
# casual-review repo itself so you can dogfood comments end-to-end.

EXT_RUN_DIR ?= $(CURDIR)

.PHONY: ext-run
ext-run: ## List the per-editor run targets
	@echo "Editor extensions — one session each ('cr' must be on PATH):"
	@echo "    make ext-vscode-run     — VS Code with the dev extension loaded"
	@echo "    make ext-jetbrains-run  — IntelliJ Community sandbox with the plugin"
	@echo "    make ext-zed-run        — instructions + open Zed at the project root"
	@echo
	@echo "Override the folder the editor opens with EXT_RUN_DIR=/path/to/repo"

.PHONY: ext-vscode-run
ext-vscode-run: ext-vscode ## Launch VS Code with the dev extension loaded (EXT_RUN_DIR overrides folder)
	@command -v code >/dev/null || { \
		echo "error: 'code' CLI not on PATH"; \
		echo "       In VS Code: Cmd-Shift-P -> Shell Command: Install 'code' command in PATH"; \
		exit 1; \
	}
	code --extensionDevelopmentPath="$(CURDIR)/$(VSCODE_DIR)" "$(EXT_RUN_DIR)"

.PHONY: ext-jetbrains-run
ext-jetbrains-run: ## Launch a sandbox IntelliJ IDEA with the plugin installed (gradlew runIde)
	cd $(JETBRAINS_DIR) && $(GRADLEW) runIde

.PHONY: ext-zed-run
ext-zed-run: ## Print Zed dev-extension install instructions; open Zed at EXT_RUN_DIR if available
	@echo "Zed has no CLI command for installing dev extensions. Steps:"
	@echo "  1. Open Zed."
	@echo "  2. Cmd-Shift-X (Extensions panel) -> 'Install Dev Extension'."
	@echo "  3. Select: $(CURDIR)/$(ZED_DIR)"
	@echo
	@if command -v zed >/dev/null; then \
		echo "Opening Zed at $(EXT_RUN_DIR) ..."; \
		zed "$(EXT_RUN_DIR)"; \
	else \
		echo "(zed CLI not on PATH; open Zed manually after installing.)"; \
	fi

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

# `make release`, `release-dry`, `release-version`: tag-only CalVer releases.
include release.mk
