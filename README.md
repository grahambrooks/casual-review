# casual-review

Ultra-fast code review CLI. Brings rustc-quality diagnostics to other languages, runs equally well on a developer workstation and in CI, and is designed for both human and agent (LLM) consumers.

Most code review tools split into a client scanner plus a server to track issues over time. `casual-review` deliberately stays a single CLI — the kind of feedback you want before a commit, in line with your workflow, without infrastructure.

## Status

Early MVP. The pipeline runs end-to-end, but the rule set is small and the project hasn't been hardened against large real-world repos yet. See [PLAN.md](PLAN.md) for the phased roadmap and [context.md](context.md) for the design thinking around git as a substrate.

## What works today

- **Languages:** Rust, Python, TypeScript, TSX, Java (via tree-sitter).
- **Rules** (15 total):
  - **Universal:** `parse-error`, `todo-marker` (TODO/FIXME/XXX), `trailing-whitespace`, `large-function` (body > 40 lines), `cognitive-complexity` (Sonar-style, threshold 15 — penalizes nesting), `debug-print` (`println!`/`dbg!`/`print()`/`console.log`/`System.out.println`/`printStackTrace` etc.), `empty-catch` (silent error swallowing), `disabled-test` (`#[ignore]`/`xit`/`it.skip`/`@pytest.mark.skip`/`@Disabled`/`@Ignore`), `assertion-free-test` (test functions with zero assertions), `hardcoded-secret` (AWS, GitHub, Slack, OpenAI, Google API keys; private key headers).
  - **Rust:** `unwrap-used` (`.unwrap()` / `.expect()`).
  - **TypeScript/TSX:** `any-type` (explicit `any`), `ts-escape-hatch` (`@ts-ignore`/`@ts-nocheck`/`@ts-expect-error`/non-null assertions).
  - **Python:** `bare-except` (`except:` without a type).
  - **Diff-aware:** `api-surface-change` — surfaces `pub` / `export` / `public` Java types / top-level `def` items added or removed in the diff (the rule a generic linter can't write).
- **Diff-aware by default:** lints what you've changed in the working tree, not the whole repo.
- **Renderers:** human (rustc-style via [ariadne](https://crates.io/crates/ariadne)) and line-delimited JSON.
- **Performance:** ~280k LOC/sec single-thread parse + rules, ~550k LOC/sec parallel; cold-startup ~6 ms.

## Install

### Homebrew (macOS / Linux)

```sh
brew install --formula https://raw.githubusercontent.com/grahambrooks/casual-review/main/Formula/casual-review.rb
```

Or, if you tap the repo:

```sh
brew tap grahambrooks/casual-review https://github.com/grahambrooks/casual-review
brew install casual-review
```

The formula installs the pre-built binary published with each GitHub release, so no Rust toolchain is required.

### Cargo (any platform)

```sh
cargo install --git https://github.com/grahambrooks/casual-review --locked
# binary lands at ~/.cargo/bin/cr
```

### From source

```sh
cargo build --release
# binary at ./target/release/cr
```

## Versioning

`casual-review` uses **CalVer** (`YYYY.M.D`) — the version is literally year, month (1-12), and day-of-month (1-31). Today's release would be `2026.4.28`. Cadence is opportunistic, not scheduled; same-day re-releases use the following day's number. See [PLAN.md](PLAN.md) for the phased roadmap.

## Usage

```sh
cr check                                  # default: working-tree diff (lints what you've changed)
cr check --staged                         # only staged changes
cr check --all                            # in changed files, lint unchanged lines too
cr check --repo                           # walk the whole repo (respects .gitignore)
cr check src/foo.rs src/bar.ts            # explicit files (no git)
cr check --verbose                        # print "checked N files, M diagnostics" to stderr
cr check --format json                    # line-delimited JSON for an agent or jq
```

If `cr check` produces no output, it's either because nothing fired (working tree clean is a common cause — the default mode lints only what's changed) or because no supported files were found. `cr check --repo --verbose` is the right command for a first evaluation pass.

Exit codes: `0` clean, `1` errors found, `2` tool failure.

`cr explain` lists all rules with one-line summaries. `cr explain <rule-id>` (e.g. `cr explain cognitive-complexity`) prints full documentation for a rule — what it catches, why it matters, how to fix it.

## Using cr with an AI agent

`cr` is designed to be useful to LLM agents (Claude Code, Cursor, etc.) as well as humans. The recommended workflow, JSON schema, rule semantics, and stability commitments are documented in [AGENTS.md](AGENTS.md). Claude Code users will find a brief orientation in [CLAUDE.md](CLAUDE.md).

In short: agents should call `cr check --format json` (or `--repo`/`--staged`), parse one diagnostic per line, prioritize by severity, and use `cr explain <rule-id>` to look up rule semantics on demand. The JSON schema is stable across patch releases within a CalVer minor.

## Tech

- **Rust** for cold-start speed and parallelism (rayon).
- **tree-sitter** for language-aware analysis. Grammars statically linked.
- **git2** (libgit2) for diff resolution and hunk-derived changed-line ranges.
- **ariadne** for the rustc-style diagnostic renderer.

## Layout

Single binary crate, subsystems as directory modules under `src/`:

```
src/diagnostic/   Diagnostic, Span, Severity, Suggestion
src/render/       human (ariadne), json
src/parse/        tree-sitter, language enum, parser pool
src/git/          working-tree + staged diff via git2
src/rules/        Rule trait + built-in rules
src/engine.rs     Orchestration (parallel via rayon)
src/cli.rs        clap definitions
```

## Project files

- [PLAN.md](PLAN.md) — phased implementation plan and open decisions
- [context.md](context.md) — design notes on git as substrate for review/agent annotations

## Releases

Releases are tag-driven. Pushing a tag matching `v*.*.*` (e.g., `v2026.4.0`) triggers `.github/workflows/release.yml`, which:

1. Verifies the tag matches `Cargo.toml`'s version.
2. Builds release binaries for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`.
3. Uploads each archive (with sha256 sidecar) to the GitHub Release.
4. Updates `Formula/casual-review.rb` with the new version + per-platform SHA256s and commits the change back to `main`.

To cut a release:

```sh
# Bump version in Cargo.toml, e.g. 2026.5.0
git commit -am "Release 2026.5.0"
git tag v2026.5.0
git push origin main v2026.5.0
```

## CI

`.github/workflows/ci.yml` runs on every push to `main` and every pull request: `cargo build`/`test` on Linux, macOS, and Windows; `cargo fmt --check`; `cargo clippy -D warnings`.

## License

Dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
