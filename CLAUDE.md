# CLAUDE.md

This file is a Claude-Code-friendly entry point. The full agent guidance lives in [AGENTS.md](AGENTS.md) — start there.

## Quick orientation

`casual-review` is an ultra-fast code review CLI. The binary is `cr`. It parses Rust / Python / TypeScript / TSX / Java with tree-sitter and emits rustc-style diagnostics for ~15 built-in rules.

## When you're using cr in someone's repo

See [AGENTS.md](AGENTS.md). The five-step workflow is:

1. `cr check --format json` (or `--repo`/`--staged` per task)
2. Parse one JSON object per line
3. Prioritize: error → warning → note
4. Decide per finding: fix / surface / skip
5. Re-run after changes

`cr explain <rule-id>` prints full documentation for any rule.

## When you're contributing TO casual-review (this repo)

Conventions and operations:

- **Build / test**: `make build`, `make test`. Or `cargo build` / `cargo test` directly.
- **Lint**: `make lint` (= `fmt-check` + `clippy -D warnings`). CI enforces both.
- **Snapshots**: `make snapshots-review` (uses `cargo insta review`). New rules need snapshot tests in `tests/snapshots.rs`; diff-aware tests go in `tests/api_surface.rs`.
- **Self-eval**: `make selfcheck` runs `cr` against this repo's `src/` so you can see what fires on your own changes before pushing.

## Architecture in one paragraph

Single binary crate. `src/diagnostic/` holds the data types. `src/parse/` wraps tree-sitter with thread-local parser pools. `src/git/` wraps libgit2 to extract diffs and HEAD blobs. `src/rules/` holds rule implementations behind a `Rule` trait — see `src/rules/util.rs` for shared helpers (especially `find_capture` which solves the recurring tree-sitter capture-by-index footgun). `src/engine.rs` drives the `discover → parse → run rules → render` pipeline in parallel via rayon. `src/render/` is human (ariadne) and JSON.

## When you add a new rule

1. New file `src/rules/<rule_name>.rs` implementing `impl Rule`. The trait requires `id()`, `run()`, and `explain()` — all three.
2. Register in `src/rules/mod.rs`: add `pub mod <rule_name>;` and a `Box::new(...)` line in `default_rules()`.
3. Add a fixture in `fixtures/eval_<rule_name>.<ext>` and a snapshot test in `tests/snapshots.rs`.
4. `make snapshots-accept` to record initial snapshots.
5. `make lint && make test` before committing.

## Things to know

- The user prefers a single binary crate with directory modules over Cargo workspaces (this is in `memory/feedback_crate_layout.md`).
- Tree-sitter `m.captures` iterates by tree position, not by capture-name index. Always use `super::util::find_capture(query, captures, name)`.
- `find_child(node, kind)` exists for the same reason — the borrow checker rejects the inline form.
- The user prefers terse responses; don't summarise diffs they can read themselves.
