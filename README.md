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

```sh
cargo build --release
# binary at ./target/release/cr
```

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

## License

MIT OR Apache-2.0
