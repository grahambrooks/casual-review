# casual-review — Implementation Plan

A phased plan for a Rust CLI that brings rustc-quality diagnostics to other languages, runs equally well on a developer workstation and in CI, and (eventually) shares findings through Git's own substrate. Pragmatic and opinionated; uncertainty is called out where it is real.

## Status snapshot — 2026-04-30

Phases 1 through 4 have shipped. The plan below is preserved as the design record; the live work is now in **Phase 5** (section 12), which prioritises functional and non-functional improvements over greenfield features.

| Phase | State | Notes |
|---|---|---|
| **1 — MVP** | ✅ shipped | 15 rules (vs planned 3), 5 languages (Rust, Python, TS, TSX, Java; vs planned 2). Diff-aware default, JSON + human renderers, exit codes. |
| **2 — CI-grade** | ✅ shipped | SARIF, GitHub format, `.casual-review.toml` config (rule disable + path glob suppression), criterion benchmarks. *Open:* CI regression alerts on bench drift (designed in §8, not yet wired in `.github/workflows/`). Go grammar still not added. |
| **3 — Substrate** | ✅ shipped | `cr publish/show/ack/fetch/push` against `refs/notes/casual-review`, schema `casual-review/finding/1`, threaded dismissals, file-based fallback when not in a git repo. See `PHASE3_NOTES.md`. |
| **4 — Collaboration** | ✅ mostly shipped | Comments CLI (`cr comment add/list/reply/resolve/reanchor`), schema `casual-review/comment/1`, ref split (`/findings`, `/discuss`), staleness via `anchor_text_sha`, ancestor projection. Editor extensions: VS Code, JetBrains, **Zed** (Zed was not in the original plan). *Open:* §11 sub-phase 4.2 smart re-anchoring, sub-phase 4.4 forge bridge. |
| **5 — Hardening & growth** | ⏳ active | See §12 for functional and non-functional priorities. |

Counts at this snapshot: ~52 unit tests + 39 integration tests across `tests/`, all green. Self-eval (`make selfcheck`) currently emits ~200 warnings on the project's own `src/` — most are `commented-code` false positives on doc-comments and `debug-print` hits in legitimate CLI output paths, both called out as Phase 5 work in §12.

## 0. Guiding principles

- **Cold start under 100 ms on a single file.** Anything slower stops being "casual."
- **Diagnostics first, rules second.** A great error renderer with three trivial lints beats ten lints rendered as `grep` output.
- **Diff-aware by default.** "Lint everything you changed" is the default verb. Whole-tree scans are an opt-in mode.
- **Two equally first-class consumers: humans at the terminal, and agents/LLMs reading stdout.** Output formats are not an afterthought.
- **Git is a substrate, not a sidecar.** The persistence layer is designed so a finding *could* survive in `refs/notes/casual-review` — but that is Phase 3, not MVP.

---

## 1. Project scaffolding

### Single-crate layout

One binary crate, with subsystem boundaries expressed as directory modules under `src/`. No Cargo workspace. Lower ceremony, one `Cargo.toml`, no inter-crate version pinning. Module visibility (`pub(crate)` etc.) does the boundary-enforcement that crate boundaries would have done.

```
casual-review/
  Cargo.toml
  src/
    main.rs                   # Binary entry: parse args, dispatch
    lib.rs                    # Re-exports for integration tests + future embedding
    cli.rs                    # clap definitions
    engine.rs                 # Orchestration: discover → parse → rules → render
    diagnostic/               # Core types
      mod.rs
      span.rs
      severity.rs
      suggestion.rs
    render/                   # Output formats
      mod.rs
      human.rs                # ariadne
      json.rs                 # serde_json
    parse/                    # tree-sitter
      mod.rs                  # Language enum, ParserPool
    git/                      # git2 wrappers
      mod.rs
      diff.rs                 # DiffSpec, ChangedFile, hunk → line ranges
    rules/                    # Built-in rules
      mod.rs                  # Rule trait, RuleCtx, registry
      parse_error.rs
      todo_marker.rs
      trailing_whitespace.rs
  tests/                      # Integration + snapshot tests (insta)
  fixtures/                   # Sample inputs and expected snapshots
  benches/                    # criterion benchmarks (Phase 2)
```

**Why this shape:**
- One `cargo build`, one version, one `Cargo.lock`.
- Module directories encode the same boundaries (`diagnostic`, `render`, `parse`, `git`, `rules`, `engine`) that the original workspace plan used. The Rule trait still abstracts rules; the engine still consumes a `DiffSpec`. The discipline is unchanged — the packaging is simpler.
- If casual-review is ever embedded (editor plugin, MCP server), `lib.rs` re-exports the types those callers need. Splitting into a separate `cr-core` crate at that point is a refactor, not a redesign.

### Crate dependencies (concrete picks)

| Concern | Pick | Rationale |
|---|---|---|
| CLI parsing | `clap` v4 with `derive` | Standard. `clap_complete` for shell completions later. |
| Errors (internal) | `thiserror` | Typed errors in `git`, `parse`, `rules` modules. |
| Errors (top level) | `anyhow` | In `main.rs` / `engine.rs` for boundary error context. |
| Diagnostic rendering | `ariadne` | The natural pick for "rustc-style." See section 3. |
| Git plumbing | `git2` | Required by README. Pin a recent version; libgit2 is the right level. |
| Tree-sitter | `tree-sitter` + per-language `tree-sitter-*` grammar crates | See section 4. |
| Concurrency | `rayon` | Embarrassingly parallel file analysis. |
| Logging | `tracing` + `tracing-subscriber` | `--verbose` and structured logs in CI. |
| Serialization | `serde` + `serde_json` | JSON output, notes payloads, fixtures. |
| File walking | `ignore` (the `ripgrep` crate) | Honors `.gitignore`. Already battle-tested. |
| Testing | `insta` for snapshot tests, `assert_cmd` for CLI | Diagnostics are output-shaped — snapshots are the right tool. |
| SARIF (Phase 2) | hand-roll via `serde` against the SARIF 2.1.0 schema | Existing crates are sparse; the format is stable. |

### Error strategy

`thiserror` for typed errors inside subsystem modules (`git`, `parse`, `rules`). `anyhow` at the top level (`main.rs`, `engine.rs`) for context-attached user-facing failures. No `unwrap` in non-test code; clippy is configured with `-D unwrap_used` in the package `[lints]` table.

### Next decisions for section 1
- **Binary name.** `casual-review` is honest; `cr` is what people will type. Ship both (`cargo install` produces `cr` as an alias).
- **MSRV.** Pin to current stable minus one. Document it.

---

## 2. MVP scope

The smallest demoable artifact that earns the project's name:

> `cr` run on a dirty working tree prints rustc-quality diagnostics for the lines you changed.

### MVP feature list

1. `cr check` — default verb. Resolves "what changed" via `cr-git` (`HEAD` vs working tree by default; `--staged`, `--against <ref>` flags).
2. Three trivial lints to exercise the pipeline end-to-end:
   - **`tree-sitter-parse-error`** — surfaces tree-sitter `ERROR` and `MISSING` nodes as diagnostics. This alone is useful and proves grammar integration.
   - **`todo-marker`** — `TODO`/`FIXME`/`XXX` in comments, with severity `note`. Detected via tree-sitter comment queries, not regex over source — this is the demo of "we understand the code, not just the bytes."
   - **`trailing-whitespace`** — byte-level lint, severity `warning`. Trivial, proves we can mix textual and AST rules.
3. Two languages at MVP: **Rust** and **Python**. Rust because we eat our own dog food; Python because grammars are mature and the user base is huge.
4. Renderer: ariadne human output. JSON output stubbed but unstable.
5. Exit code: `0` if no error-severity diagnostics, `1` otherwise. (See section 7 for the full table.)

### Out of MVP, explicitly

- Notes-based persistence.
- SARIF output.
- More than two languages.
- Custom user rules.
- Incremental parsing.
- Any server component.

### MVP acceptance test

A fixture repo with known-bad files; `cr check` prints diagnostics whose textual rendering matches a snapshot. CI runs the same fixture and asserts identical output. Total wall time on the fixture: under 200 ms on an M-class laptop.

---

## 3. Diagnostic engine

This is the core asset. It must be rich enough to render rustc-quality output and serializable enough to round-trip through JSON, SARIF, and (later) a notes payload.

### Type sketch (in `cr-core`)

```rust
pub struct Diagnostic {
    pub code: DiagnosticCode,        // e.g. "CR0001" or "todo-marker"
    pub severity: Severity,          // Error, Warning, Note, Help
    pub message: String,             // Primary one-line message
    pub primary: Span,               // Where the squiggle goes
    pub labels: Vec<Label>,          // Secondary spans with their own messages
    pub notes: Vec<String>,          // Footer notes ("note: ...")
    pub helps: Vec<String>,          // Footer helps ("help: ...")
    pub suggestions: Vec<Suggestion>,// Machine-applicable fixes
    pub source: RuleId,              // Which rule produced this
}

pub struct Span {
    pub file: PathBuf,               // Repo-relative
    pub byte_range: Range<usize>,    // For ariadne and applying suggestions
    pub line_col_start: (u32, u32),  // 1-based; for human-readable output
    pub line_col_end: (u32, u32),
}

pub struct Suggestion {
    pub message: String,             // "consider replacing with..."
    pub applicability: Applicability,// MachineApplicable, MaybeIncorrect, HasPlaceholders
    pub edits: Vec<TextEdit>,        // Multi-edit suggestions are first-class
}
```

Suggestions explicitly model rustc's `Applicability` enum because that distinction matters for any future `cr fix` subcommand and for any agent consuming the output.

### Renderer: ariadne vs codespan-reporting

**Recommendation: ariadne.**

- ariadne renders multi-span diagnostics with arrows, fish-bones, and color in the rustc idiom out of the box. That is the explicit goal of this project.
- codespan-reporting is more conservative; its output is clean but flatter. It is the right pick if you want LSP-style diagnostics, not rustc-style.
- Risk: ariadne is maintained but smaller than codespan-reporting. Mitigation: `cr-render` hides the renderer behind an internal trait so swapping is mechanical.

### Output formats (design, not all in MVP)

- `--format human` — ariadne. Default when stdout is a TTY.
- `--format json` — line-delimited JSON, one object per diagnostic. Stable schema, versioned via `"schema": "casual-review/1"`.
- `--format sarif` — SARIF 2.1.0. Phase 2.
- `--format agent` — a compact, deterministic text format optimized for LLM consumption: one diagnostic per block, no ANSI, fixed field order, no decorative borders. This is the format an agent pipes through. Worth treating as first-class — see section 10.

### Next decisions for section 3
- **Diagnostic codes.** Numbered (`CR0001`) like rustc, or named (`todo-marker`)? Numbered scales but requires a registry; named is friendlier. Lean named, with stable slugs and an internal registry to prevent drift.
- **Stability commitment for JSON output.** Worth committing to before the first non-MVP release; downstream tools will pin to it.

---

## 4. Tree-sitter integration

### Grammar loading

**Statically link grammars at MVP.** Each grammar is a `tree-sitter-<lang>` crate with C sources; `cargo build` compiles them in. No runtime grammar discovery, no dynamic loading, no `.so` files.

Trade-off acknowledged: every new language adds compile time and binary size. That is acceptable for the MVP language set (2–6 languages). Dynamic loading via WASM grammars is a Phase 3 conversation — see section 9.

### Parser cache

Tree-sitter parsers are cheap to create but not free, and `Tree` objects are reusable. `cr-parse` exposes:

```rust
pub struct ParserPool { /* one Parser per language, per thread */ }
pub fn parse(&self, lang: Language, src: &[u8], old: Option<&Tree>) -> Tree;
```

Per-thread pools (via `thread_local!` or rayon's `ThreadLocal`) avoid contention. The `old` parameter is the hook for incremental parsing; not used in MVP but designed in.

### How rules are authored

Two mechanisms, deliberately:

1. **Tree-sitter queries** for pattern-matchable rules (`(comment) @c (#match? @c "TODO|FIXME")`). Authored as `.scm` files under `crates/cr-rules/queries/`. Cheap to add, easy to read, no recompile if loaded at runtime — though MVP loads them via `include_str!`.
2. **Rust `Rule` trait impls** for anything queries cannot express (control-flow, cross-file, semantic checks). The trait:

```rust
pub trait Rule {
    fn id(&self) -> RuleId;
    fn languages(&self) -> &[Language];
    fn run(&self, ctx: &RuleCtx<'_>) -> Vec<Diagnostic>;
}
```

`RuleCtx` carries the parsed tree, source bytes, file path, and the changed line ranges (from `cr-git`). A rule that wants to be diff-aware filters its own results against the change set; the engine offers a helper.

### Tree walking

For query-based rules, use `tree_sitter::QueryCursor`. For Rust-coded rules, walk the tree with `TreeCursor` — avoid recursion to keep stack predictable and visitor allocation minimal.

### Next decisions for section 4
- **Initial language set.** Beyond MVP's Rust + Python: TypeScript, JavaScript, Go are the obvious next four. Picking based on user demand vs grammar maturity is a real call.
- **Query authoring convention.** Bundle queries with their rule, or share a query library across rules? Lean toward per-rule for now; consolidate if duplication appears.

---

## 5. Git integration

`cr-git` wraps `git2` and exposes a small, opinionated API:

```rust
pub enum DiffSpec {
    WorkingTree,                       // HEAD vs working tree (default)
    Staged,                            // HEAD vs index
    Unstaged,                          // index vs working tree
    Range { base: String, head: String }, // e.g. main..HEAD
    Commit(String),                    // a single commit's diff against its parent
}

pub struct ChangedFile {
    pub path: PathBuf,
    pub status: FileStatus,            // Added, Modified, Deleted, Renamed
    pub new_content: Option<Vec<u8>>,  // Bytes to feed tree-sitter
    pub changed_line_ranges: Vec<Range<u32>>, // 1-based, in new file
}

pub fn changed_files(repo: &Repository, spec: &DiffSpec) -> Result<Vec<ChangedFile>>;
```

Key design points:

- **The diff returns content, not just paths.** Working-tree diffs read from disk; staged diffs read from the index blob; range diffs read from the head tree. The engine never touches `git2` directly.
- **Line ranges are computed from hunk headers.** These feed `RuleCtx::changed_lines`, so rules can scope their output to "what the developer touched." This is the heart of "diff-aware lints."
- **Renames are followed.** `git2::DiffFindOptions::renames(true)`.
- **No working-tree mutation, ever.** `cr-git` is read-only on the repo. (Notes writes in Phase 3 are an explicit, opt-in exception.)

### Mapping changed lines onto tree-sitter results

The engine intersects `changed_line_ranges` with each diagnostic's `Span`. A diagnostic is "in scope" if any of its bytes overlap a changed range. Default behavior:

- Errors and warnings *anywhere in changed files* are reported.
- Notes/helps are reported only when their span is inside a changed line range. (Otherwise an unchanged file lights up with `// TODO` notices for code the user did not touch.)

`--all` overrides this and reports everything. `--changed-only` is the strictest mode and filters errors/warnings to changed lines too.

### Next decisions for section 5
- **Submodules.** Recurse, skip, or warn? Default skip is safe; revisit when someone asks.
- **Worktrees.** `git2` handles them, but the discovery story (`cr` run from a linked worktree) needs an integration test.

---

## 6. Persistence and sharing of findings — Phase 3

Direct nod to context.md. This is the design target, not the MVP.

### Goal

Optional, opt-in persistence of findings to a custom notes ref so they can travel with the repo, be reviewed by an agent or human later, and be cleared without rewriting history.

### Proposed scheme

- **Ref:** `refs/notes/casual-review`. Single ref to start; split into sub-refs (`casual-review/findings`, `casual-review/discuss`) only if the second kind appears.
- **Attachment object:** the commit being reviewed (typically `HEAD` or the head of the branch under review). Findings about uncommitted work are not persisted — they only exist on stdout. If the user wants persistence they must commit first. This is a feature, not a limitation: it keeps the substrate honest.
- **Note payload:** JSON, schema versioned, modeled on git-appraise's `analyses` shape but specialized for rich diagnostics.

```json
{
  "schema": "casual-review/finding/1",
  "tool": "casual-review",
  "tool_version": "0.x.y",
  "produced_at": "2026-04-27T12:00:00Z",
  "commit": "abc123...",
  "findings": [
    {
      "id": "CR-<hash>",
      "rule": "todo-marker",
      "severity": "note",
      "location": {
        "file": "src/lib.rs",
        "byte_range": [1024, 1028],
        "line_range": [42, 42],
        "col_range": [5, 9]
      },
      "message": "...",
      "labels": [...],
      "suggestions": [...],
      "parent": null
    }
  ]
}
```

`parent` exists so future replies/dismissals can thread the same way git-appraise threads comments. Out of scope for v1 of the schema, but reserved.

### CLI surface (Phase 3)

- `cr publish` — write current findings to `refs/notes/casual-review` on `HEAD`.
- `cr show` — read findings for a commit (local or fetched).
- `cr fetch` / `cr push` — convenience around `git fetch/push refs/notes/casual-review:refs/notes/casual-review`.
- `cr ack <id>` — append a "dismissed" note threaded by `parent`. Findings are not deleted; appending is the mutability story, exactly like git-appraise.

### Why this is Phase 3
- It is the area most likely to attract scope creep (auth, server, forge integration). MVP must ship before any of that conversation starts.
- It depends on the diagnostic schema being stable, which won't be true until real users have produced output for a while.

### Next decisions for section 6
- **Single ref vs multi-ref.** Single is simpler; multi mirrors git-appraise. Lean single until a second kind exists.
- **Whether to include source bytes inline.** Helps offline rendering; bloats notes. Probably no — the renderer can read the blob.

---

## 7. CI mode

CI is not a separate mode; it is `cr check` with different defaults. The CLI detects `CI=true` and `GITHUB_ACTIONS=true` and adjusts.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | No diagnostics at error severity |
| 1 | One or more error-severity diagnostics |
| 2 | Tool-level failure (couldn't open repo, grammar load failed, etc.) |
| 3 | Configuration error (bad CLI args, bad config file) |

`--max-severity warning` promotes warnings to error-for-exit-purposes without changing rendering. `--no-fail` always exits 0 unless code 2/3.

### Output formats in CI

- Default in CI: `--format json` with `--no-color`. Easy to parse, easy to pipe to `jq`.
- `--format sarif` for native GitHub code scanning ingestion. Phase 2.
- `--format github` (Phase 2): emits `::error file=...,line=...,col=...::message` workflow commands so diagnostics surface as PR annotations without SARIF. Cheap to add and high-impact for GitHub users.

### Composability examples

- GitHub Actions: a single step `cr check --against origin/${{ github.base_ref }} --format github`.
- GitLab: `--format json | jq` into Code Quality report shape; document the conversion rather than ship it.
- Pre-commit: `cr check --staged --max-severity warning`.

### Next decisions for section 7
- **Commit to SARIF?** It's the lingua franca for code scanning and unlocks GitHub's UI for free. Worth committing to as a Phase 2 deliverable. Calling this out as one of the four-to-six decisions in section 10.

---

## 8. Performance

### Budget

End-to-end wall-clock targets (cold cache, M-class laptop):

- Cold `cr check` on a single-file change in a 10k-file repo: **under 150 ms**, p95.
- Cold `cr check --all` on the casual-review repo itself: **under 500 ms**.
- Per-file analysis (parse + all rules): **under 5 ms** for files under 2k lines.

### LOC throughput target

The project's headline benchmark, comparable across machines:

| Mode | Target | MVP measured (M-class laptop) | Notes |
|---|---|---|---|
| **Single-thread parse + 3 MVP rules** | **≥ 250,000 LOC/sec** | **~280,000 LOC/sec** ✅ | ~4 µs/line. Tree-sitter alone parses Rust at roughly 30–50 MB/s; the rule layer must not add more than 2× overhead. |
| **8-core parallel, full pipeline** | **≥ 1,500,000 LOC/sec** | **~550,000 LOC/sec** ⚠ | The "casual" headline. A 1M-line repo finishes in well under a second. The MVP gap is mostly rayon task granularity (small files, per-thread parser-init not yet amortized across runs) — closeable with parser pooling and file batching, not a fundamentally different architecture. |
| **Cold-startup overhead** | **≤ 30 ms** | **~6 ms** ✅ | Above any actual work. Below this and `cr` feels instantaneous on small inputs. |

Why these numbers: ruff (Python linter, Rust-implemented) sets the bar at ~500k–2M LOC/sec depending on rule mix and hardware. Matching its parallel ceiling is the long-term aspiration; the single-thread number is what casual-review must deliver on day one. Falling materially below would make the project's "ultra fast" framing hollow.

How to measure: `criterion` benchmark (`benches/throughput.rs`) over a fixture corpus, reporting `LOC/sec` for three configurations: parse-only, rules-only-over-pre-parsed-tree, and the full parallel pipeline. Regression alerts in CI when any drops > 10%.

These are aspirations, not guarantees. They drive concurrency and caching choices.

### Concurrency model

- File walking is sequential (`ignore::WalkBuilder`).
- Per-file work (parse + run all rules) is parallel via `rayon::par_iter`.
- Rule execution within a file is sequential — the parse cost dominates and rules are cheap; parallelizing them would just add coordination overhead.
- `cr-git` operations stay on the main thread; libgit2 is not thread-safe per-repo.

### Parser reuse

Per-thread `ParserPool` (section 4). One `Parser` per language per worker thread, kept alive for the run.

### Incremental parsing

Designed in (`parse(..., old)`), unused in MVP. The natural use case: an editor integration that reparses on keystroke. CLI invocations are stateless, so incremental parsing buys little there. Revisit when LSP mode becomes a real feature.

### Caching

No on-disk cache in MVP. The temptation will be to cache parses keyed by blob SHA; resist until measurement justifies it. A cold run that already meets budget does not need a cache.

### Next decisions for section 8
- **Whether to ship a benchmark suite.** `criterion` against the fixture corpus, run in CI, with regression alerts. Recommended; cheap to add early and expensive to retrofit.

---

## 9. Extensibility

### Adding a language

A new language is:

1. Add `tree-sitter-<lang>` to `cr-parse/Cargo.toml`.
2. Register it in the `Language` enum and the parser factory.
3. Map file extensions in `cr-engine`.
4. Optionally: add per-language queries for existing language-agnostic rules (the `todo-marker` rule needs a comment query per language).

That's it. The diagnostic engine and rule trait are language-agnostic.

### Adding a rule

A new rule is either:

- A `.scm` query file plus a registration entry, or
- An `impl Rule for MyRule` plus a registration entry.

Configuration (enable/disable, severity overrides) lives in `casual-review.toml` at the repo root. Schema:

```toml
[rules]
"todo-marker" = "off"
"trailing-whitespace" = "warning"   # severity override

[languages]
include = ["rust", "python"]
```

### Plugin model

**Phase 1: compiled-in rules only.** This is the right call. It keeps cold start fast, makes rule authoring a normal Rust contribution, and avoids designing a plugin ABI before the core types stabilize.

**Phase 3 candidate: WASM rules.** Tree-sitter already has WASM grammar support; WASM rules would let third parties ship lints without a casual-review release. The cost is real (sandboxing, ABI design, perf hit) and should be deferred until at least one external team is asking for it.

**Phase 3 candidate: external-process rules.** A `cr` rule could be "run this binary on stdin, parse JSON diagnostics from stdout" — this is the LSP-flavor model. Lower-overhead than WASM to design; higher per-invocation cost. Worth keeping in mind.

### Next decisions for section 9
- **Config file format.** TOML for parity with Cargo. (`casual-review.toml`.)
- **Whether config can live in `Cargo.toml` `[package.metadata.casual-review]`.** Mild yes — costs little.

---

## 10. Risks and open questions

The four-to-six real decisions, surfaced explicitly:

1. **ariadne vs codespan-reporting.** Recommendation: ariadne for rustc-style. Reversible via `cr-render`'s internal trait, but committing now is correct. Decide at the start of MVP work.

2. **Initial language set beyond Rust + Python.** TypeScript and Go are the obvious next two. Each grammar adds compile time and binary size. The decision is partly strategic (which user base to court first) and partly grammar-quality based. Decide before Phase 2.

3. **SARIF as a first-class output.** Yes or no. SARIF unlocks GitHub code scanning and is the only standardized format here. The cost is non-trivial — the spec is large — but the payoff is direct CI value. Recommend committing to SARIF for Phase 2; treat as the gate for "this is real CI tooling."

4. **Agent-facing output as a first-class format.** The README explicitly names agents as a target audience. If agent output is just "JSON, but stable," that is Phase 1. If it is its own optimized text format (`--format agent`), that is a design exercise in itself. Recommend: ship JSON in MVP; design `--format agent` after seeing how agents actually consume `cr` output. Do not ship a half-designed agent format.

5. **Persistence in git notes — when to commit.** The design in section 6 is sound, but committing to it locks in a JSON schema. The risk is shipping a v1 schema that doesn't survive contact with reality. Mitigation: gate Phase 3 on at least 30 days of MVP usage so the diagnostic schema settles first.

6. **Whether to define `cr fix`.** Suggestions are designed in; an `apply` subcommand is a small additional surface but a large UX commitment (what does it mean for an agent to apply a `MaybeIncorrect` suggestion?). Defer past MVP, but the type design must support it from day one — and section 3's `Suggestion` type does.

### Lower-tier risks worth naming

- **libgit2 quirks** with submodules, sparse checkouts, and partial clones. Surface area is large; punt with clear error messages on first encounter rather than pre-solving.
- **Tree-sitter version drift across grammars.** Different `tree-sitter-*` crates pin different `tree-sitter` versions. Pin at the workspace level and verify on each grammar bump.
- **Windows.** libgit2 and tree-sitter both work on Windows but ariadne's Unicode rendering can surprise. CI on Windows from day one or document it as not yet supported.
- **MSRV churn.** `tracing` and `clap` push MSRV forward periodically; pin and document.

---

## 11. Collaborative comments — Phase 4

A natural extension of Phase 3's substrate: human-authored review comments that travel through git the same way findings do. The premise is unchanged — git is the transport, not a sidecar — but the producer is a person (or an editor on their behalf) instead of a rule. The first surfacing client is a VS Code extension; everything below is editor-agnostic, with the extension as a thin shell over the CLI.

### Goal

Two developers with `cr` installed can leave threaded comments on lines, files, or commits of any reachable commit, sync them via `git fetch/push`, and view/reply from inside their editor. Forge integration (mirror to GitHub/GitLab PR threads) is an optional later bridge, not a dependency.

### Substrate

Section 6 already anticipated this: "split into sub-refs only if the second kind appears." That moment is here.

- **Findings ref** migrates from `refs/notes/casual-review` → `refs/notes/casual-review/findings`. Schema unchanged.
- **Comments ref** is new: `refs/notes/casual-review/discuss`, schema `casual-review/comment/1`.

Why split: lifecycle differs (findings are regenerable from rules; comments are not), and authoring rate differs (comments may be high-frequency during active review while findings are republished in batch). One ref per kind keeps fetch/push semantics clean and avoids forcing every consumer to filter a union schema.

Migration: a one-shot `cr migrate-refs` (also run lazily on first Phase 4 command) does `git update-ref refs/notes/casual-review/findings refs/notes/casual-review` and deletes the old ref. Phase 3 is fresh enough that the cost is low; the long-term shape is worth it.

### Comment schema

```json
{
  "schema": "casual-review/comment/1",
  "tool": "casual-review",
  "tool_version": "0.x.y",
  "commit": "abc123...",
  "comments": [
    {
      "id": "CRC-<hash>",
      "author": { "name": "Graham Brooks", "email": "graham@..." },
      "created_at": "2026-04-30T12:00:00Z",
      "anchor": {
        "file": "src/lib.rs",
        "line_range": [42, 44],
        "byte_range": [1024, 1098],
        "anchor_text_sha": "<sha256 of the anchored bytes>"
      },
      "body": "Why does this allocate?",
      "parent": null,
      "resolved": false,
      "resolved_by": null,
      "resolved_at": null
    }
  ]
}
```

Notes on the shape:

- **`anchor.anchor_text_sha`** is the staleness primitive. When rendering a comment, hash the current bytes at the anchor's `byte_range`; if it differs from `anchor_text_sha`, the comment is *stale* — still shown, but flagged.
- **`parent`** threads replies, exactly like findings dismissals. A reply is a comment whose `parent` is another comment's `id`.
- **`resolved`** is a soft state, set by appending a new comment record with `parent: <original_id>` and `resolved: true` (append-only, audit-trail preserving — same model as `cr ack`). Records are never mutated.
- **`author`** is read from `git config user.name`/`user.email`. No new identity system in v1; commits already carry these values, so reusing them is consistent.
- **File-level comments** use `line_range: [0, 0]`. **Commit-level comments** omit `anchor.file`.
- **`id`** is a stable hash of `(author.email, created_at, anchor, body)` — deterministic enough to dedupe across re-fetches without needing UUIDs.

### CLI surface

```
cr comment add <FILE> --lines <START[:END]> [--body <TEXT>] [--commit HEAD]
cr comment add <FILE> --file-level [--body <TEXT>] [--commit HEAD]
cr comment add --commit-level [--body <TEXT>] [--commit HEAD]
cr comment list [--commit HEAD] [--file <FILE>] [--format human|json] [--include-resolved]
cr comment reply <COMMENT_ID> [--body <TEXT>]
cr comment resolve <COMMENT_ID> [--message <TEXT>]
cr comment reanchor <COMMENT_ID> --lines <START[:END]>
```

If `--body` is omitted, `cr` opens `$EDITOR` (same convention as `git commit`).

`cr fetch`/`cr push` sync **both** refs by default; a `--ref findings|comments|all` flag scopes when needed. Implementation is one extra refspec on the existing wrappers — no new transport code.

### Anchoring across edits

Comments survive small edits as long as the anchor bytes still hash equally; otherwise they fall into a "stale" bucket. The editor surfaces them in two places:

1. **Inline gutter marker** when `anchor_text_sha` matches the current byte range.
2. **Stale panel** when it does not — the developer can `reanchor` or `resolve`.

We deliberately do *not* implement smart re-anchoring (3-way diff, blame walking) in v1. Flag-and-prompt is honest about the limits and avoids wrong-but-confident anchoring. Smart re-anchoring is a candidate for 4.2 if friction appears.

### VS Code extension

Thin by design: it shells out to `cr` and renders. No comment storage, no JSON parsing of notes refs, no git operations of its own. The binary stays the single source of truth, and the same protocol works for any editor that can spawn a subprocess.

**Surface:**
- Gutter markers on commented lines.
- Hover/peek shows thread with a reply box.
- Command palette: `Casual Review: Add Comment`, `Casual Review: Sync` (= `cr fetch && cr push`), `Casual Review: Show Stale`.
- Status bar: count of unresolved comments on current commit.

**Wire protocol:** plain JSON over `cr comment list --format json`. No LSP, no IPC daemon. Each editor action is one `cr` invocation; the ≤30 ms cold-start budget from section 8 keeps it responsive.

**Refresh model:** the extension watches `.git/refs/notes/casual-review/discuss` (and `packed-refs` mtime) via the OS file watcher. On change, re-run `cr comment list`. No polling, no daemon.

**Out of scope for the v1 extension:**
- Authoring while offline against a teammate without `cr` installed.
- Inline diff rendering of comment threads.
- Authentication beyond `git` itself.

### Forge bridge — 4.4 (optional)

A later `cr bridge github push <PR>` mirrors comments to PR review threads via the GitHub API (and inverse for fetch). The substrate stays authoritative; the forge is a view. Gate on real demand and a clear two-way conflict story before designing.

### Sub-phasing within Phase 4

| Sub-phase | Scope | Exit criteria |
|---|---|---|
| **4.1 — comment CLI** | Ref split + migration, schema v1, `cr comment add/list/reply/resolve`, fetch/push extension | Two clones leave and read each other's comments via `cr push`/`cr fetch` |
| **4.2 — anchoring polish** | Stale detection, `reanchor`, evaluation of smart re-anchor | Stale rate manageable on a real repo (target: <20% per week of active dev) |
| **4.3 — VS Code extension** | Gutter, peek, sync command, refresh on save (no daemon) | Authoring and reading comments end-to-end from the editor on a real PR. Scaffolded under `extensions/vscode/`; compiles via `npm run compile`. |
| **4.4 — forge bridge (optional)** | GitHub mirror, both directions | One real PR has comments authored in `cr` showing up on github.com |

### Next decisions for section 11

- **Per-comment vs per-thread `id`.** Lean per-comment; threads derive from `parent`. Mirrors findings. Decided.
- **`cr comment add` requires a committed anchor.** Yes — same rule as `cr publish`. Comments on uncommitted work live only in the editor's transient state. Keeps the substrate honest.
- **Default visibility of comments on ancestor commits.** When viewing `HEAD`, show comments anchored to ancestors whose `anchor_text_sha` still matches the current bytes; hide drifted ones unless `--all-commits`.
- **Anchor identity (bytes vs. line content).** Bytes are unambiguous; line content is robust to leading-whitespace edits. Recommend bytes for v1; revisit if whitespace-only edits cause excessive staleness.
- **Editor-agnostic protocol.** Decide early whether the JSON shape returned by `cr comment list --format json` is a stable contract. Recommend yes — it is the editor extension API, and the JetBrains/Neovim ports we want next month will pin to it.

---

## 12. Phase 5 — Hardening & growth

Phases 1 through 4 shipped breadth: pipeline, rules, persistence, comments, three editor extensions. Phase 5 is depth — make the surface that exists actually pleasant to use, then grow it where the cost is justified.

The work splits into two threads. Functional improvements raise what `cr` can do for a user; non-functional improvements raise how reliably and unsurprisingly it does it. They run in parallel; neither is a prerequisite for the other except where called out.

### 12.1 Functional improvements

#### F1. Rule precision (highest priority)

`make selfcheck` currently emits ~200 warnings on the project's own `src/`. The breakdown reveals where the rules misfire:

- **`commented-code` (109 hits, ~80% false-positive estimate)** — Rust `///` and `//!` doc comments and TS/Java `/** */` JSDoc/Javadoc are being treated as commented-out code because they often contain code-shaped text (`fn`, `let`, parentheses). Fix: skip doc-comment node kinds before running the heuristic. Tree-sitter exposes `doc_comment` separately in Rust; in TS/TSX inspect for `/**` prefix. Add fixtures that lock the false-negative behaviour in too.
- **`debug-print` (36 hits, mostly legitimate)** — `cr` itself is a CLI; `println!` in `src/main.rs`, `src/notes_io.rs` and similar is intentional output, not debug noise. Two complementary fixes:
  1. **Suppression directives in source.** `// cr-allow debug-print` (next-line) and `// cr-allow-file debug-print` (whole file) — borrowed from `eslint-disable` and `clippy::allow`. Implementable as a query over comments + a span-overlap check. This is also the right answer for legitimately-flagged code that the author has consciously accepted.
  2. **`.casual-review.toml` glob-scoped rule overrides** — already partially supported via the `suppress.paths` global list; extend to per-rule path scoping (`[rules."debug-print"]\nsuppress_paths = ["src/main.rs"]`).
- **`unwrap-used` (19 hits)** — regex constants and post-insert `HashMap::get` are intentional. Same suppression-directive answer.

**Exit criterion for F1:** `make selfcheck` produces zero warnings without disabling rules wholesale. Either the rule fixes the false positive, or the source has a justified suppression directive.

#### F2. Suppression directives (`cr-allow`)

Inline source pragmas, modelled on `// clippy::allow(...)` and `# noqa: E501`:

```rust
// cr-allow: debug-print  -- top-level CLI output
println!("...");

#[allow_attribute_ish] // cr-allow-next-line: unwrap-used
let x = compile_regex().unwrap();
```

```python
# cr-allow: debug-print
print("...")
```

```typescript
// cr-allow: any-type, ts-escape-hatch
const x: any = ...; // @ts-ignore
```

Implementation outline:
- Comment query per language → extract `cr-allow`/`cr-allow-next-line`/`cr-allow-file` directives.
- Build a per-file suppression map keyed by `(line, rule_id)`.
- After rule runs, filter diagnostics whose `(line, code)` is suppressed.
- New rule: `unused-allow` — surface directives that suppressed nothing. Good citizens delete dead suppressions.

Decided early: suppression takes a comma-separated rule list, not a wildcard. `cr-allow: *` is never a good idea in source.

Exit criterion: every legitimate self-eval finding either disappears at the rule level (F1) or carries a `cr-allow` with a one-line justification.

#### F3. `cr fix` — apply suggestions

The diagnostic type already carries `suggestions: Vec<Suggestion>` with `Applicability` per rustc. What's missing is the apply path:

- `cr fix` — apply all `MachineApplicable` suggestions in changed files; print a summary of what changed. Default refuses to touch unsaved buffers (working-tree diffs `cr` itself produced).
- `cr fix --suggest <severity>` — also apply `MaybeIncorrect` suggestions, but write a `.cr-fix-backup` next to each file and require `--force` if the working tree is dirty.
- `cr fix --apply <CR-id>` — apply one specific finding's suggestion.

Most rules don't yet emit suggestions. Start with the trivially-fixable ones:
- `trailing-whitespace` → strip
- `todo-marker` → no auto-fix; never makes sense
- `commented-code` → no auto-fix (we don't know if delete or restore)
- `unwrap-used` → suggest `?` if function returns `Result`; otherwise no auto-fix
- `any-type` → suggest `unknown` (mechanical replacement, MaybeIncorrect)
- `ts-escape-hatch` → no auto-fix; the comment is the warning

Exit criterion: `cr check && cr fix && cr check` is idempotent and leaves the tree in a state that compiles.

#### F4. `--format agent`

§3 designed this and §10 deferred it to "after seeing how agents actually consume `cr` output." After 6+ months of agent usage via `--format json`, the patterns are clear enough to design:

- One diagnostic per record, no ANSI, fixed field order, no decorative borders.
- Includes a one-line `code-block` field showing the offending source line(s) inline so the agent doesn't have to re-read the file.
- Drops `byte_range` (agents don't seek by byte) and `col_*` defaults (line is enough).
- Surfaces `helps[0]` as a prominent `fix:` field — most rules' `helps` is the actionable sentence.

Compare against `--format json` (which stays the editor extension's contract — stable, fully-typed, byte-accurate) and `--format human` (ariadne, for terminals).

Exit criterion: round-trip with Claude Code or similar — agent reads `--format agent`, applies fixes, re-runs, finishes with zero diagnostics on a synthetic fixture.

#### F5. Smart re-anchoring (Phase 4.2 follow-through)

§11 deferred this past v1 of comments behind "flag-and-prompt is honest." That stance holds for v1; the question is when it costs a real user enough friction to justify. Build the metrics first:

- `cr comment list --include-stale --format json` already surfaces stale comments. Add a `--stats` flag that reports the staleness rate over a window (last 30 days of commits, say).
- If the rate is >20% on an active repo (the §11 guess at the friction threshold), implement re-anchoring against the comment's anchor text:
  1. Search for the original anchor text within the new file (Levenshtein-bounded).
  2. If exactly one match, propose a re-anchor with `cr comment reanchor --auto`.
  3. If zero or many, leave stale.

The expensive option (3-way diff via blame walk) is out of scope until single-match search proves insufficient.

Exit criterion: stale rate measured on at least one real repo with an active comment thread; if it crosses the threshold, re-anchor lands; if not, this stays deferred.

#### F6. Phase 4.4 — forge bridge (optional)

`cr bridge github push <PR>` mirrors comments to PR review threads via the GitHub API; the inverse pulls. Substrate stays authoritative; the forge is a view.

Gate on: at least one user asking for it, plus a clear two-way conflict-resolution story (forge → ref or ref → forge wins on collision). Until both arrive, this stays as a sketched intent. The cost — auth, REST surface, conflict logic — is large and best paid once.

#### F7. More languages — selectively

The plan called for Go after TypeScript. It hasn't shipped because Java (added) was higher-leverage for Java-shop adoption and the Go grammar adds non-trivial compile time + binary size for a language `cr` doesn't yet have native rules for.

Pick by demand, not by completeness:
- **Go** — high LOC universe; mature grammar; would add another language for `unwrap-equivalent` (`if err != nil` boilerplate density is the analogue rule).
- **JavaScript** (separate from TS) — already half-done via `tree-sitter-typescript` parse-fallback; a clean Language::JavaScript would unlock plain JS files.
- **C / C++** — large, but grammars are mature. Holds the door open for kernel/embedded reviewers.
- **Kotlin** — Java-shop adjacent; small incremental cost given Java is in.

For each addition, the bar is one new rule that's *language-specific* enough to demonstrate value (i.e., not just `todo-marker` rebadged). Without that, the extra grammar is dead weight.

#### F8. Cross-file analysis (one rule, opt-in)

§9 said "no cross-file analysis in v1." That holds, but a single high-value rule is worth spiking:

- `unused-public` — public symbols (Rust `pub`, TS `export`, Java `public class`) with zero references anywhere in the working tree. Diff-aware: only fires for symbols added in the diff that are also unreferenced.

This requires a per-run symbol index; the cost is real (a second pass before rules run, doubling parse work in the worst case). Treat as opt-in via `--rule unused-public` for now; if the rule becomes universally desired, lift it into the default set and amortise the index across rules that want it.

Decided up-front: this rule is the only cross-file rule until measurement says otherwise. `unused-import`, `unused-export`, `dead-code` all ride on the same index but are deferred — `unused-public` proves the index pays for itself first.

#### F9. More rules (incremental)

Filed in priority order; each is a separate small PR:

1. **`long-parameter-list`** — function signatures with > 6 parameters. Universal. Sonar-style.
2. **`magic-number`** — numeric literals not in `(-1, 0, 1, 2)` outside `const` and test code. Lots of false positives possible; needs `cr-allow` (F2) to be useful.
3. **`identical-branches`** — `if`/`else` arms with identical bodies. Catches a real bug class.
4. **`shotgun-surgery`** — file with > N changed regions in a single diff. Emits a *note*, not a warning — heads-up for reviewers about a sprawling change.
5. **`returns-from-finally`** — Java/TS rule for `return` in `finally` blocks. Tiny but always a real bug.

Anything past these waits for a request.

#### F10. Neovim extension

The §11 `cr comment` JSON contract is editor-agnostic by design. Three extensions exist (VS Code, JetBrains, Zed); a Neovim port reuses the same protocol. Estimated cost: small Lua plugin shelling out to `cr`, gutter signs via `nvim-treesitter` siblings, telescope picker for comment list. Keep deferred until at least one regular Neovim user asks; the work is small but the support burden of a fourth editor is non-trivial.

### 12.2 Non-functional improvements

#### N1. Self-eval cleanliness

Bound to F1 + F2 above but worth tracking separately as a CI gate. Add `make selfcheck` (already exists) to CI as a **non-blocking** check today, then promote to **blocking** once F1+F2 land. Adopt the rule: `cr` itself must produce zero warnings on its own `src/`. Deviating means either fixing the rule or annotating the source — never silently ignoring.

This also puts the project in the position of being its own most demanding user, which is the cheapest way to keep the rules honest.

#### N2. Performance — close the parallel-pipeline gap

§8 measured single-thread at ~280k LOC/sec (target ≥250k — ✅ met) and 8-core parallel at ~550k LOC/sec (target ≥1.5M — ⚠ 36% of target). The diagnosis from §8: rayon task granularity, per-thread parser-init not amortised across runs, file batching.

Concrete attack:

1. **Parser-pool warm-up.** Allocate one parser per language per worker thread once at engine startup (`rayon::ThreadPoolBuilder::start_handler`), not lazily on first parse. Saves the ~200µs grammar-init per-thread cost on cold runs.
2. **Batch small files.** Files under ~500 LOC parse so fast that rayon's work-stealing overhead dominates. Batch 8 such files per task before parallelism kicks in.
3. **Profile.** `cargo flamegraph` on the bench corpus — no further optimisations until the flamegraph picks the next bottleneck. Conjectures don't beat profilers.

Exit criterion: parallel ≥ 1.0M LOC/sec on the same fixture. Hitting 1.5M is the stretch goal; 1.0M closes the most-embarrassing gap and earns "ultra-fast" credibly enough.

#### N3. CI bench-regression gate

§8 specified "regression alerts in CI when any drops > 10%" and the criterion bench (`benches/throughput.rs`) is implemented, but no CI workflow runs it. Wire it:

- Add a `bench` job to `.github/workflows/ci.yml` running on a stable runner (Ubuntu, single concurrency). Report current LOC/sec for each of the three configurations (parse-only, rules-only, full pipeline).
- Persist results as an artifact; compare to the previous run on `main`. Fail with > 10% regression in any single metric.
- Tolerance for noise on shared GitHub runners is wide; an alternative is a self-hosted runner. Start with Ubuntu-latest and a 15% tolerance; tighten if noise allows.

This is also the gate behind which N2 progress is measurable — without N3, performance changes are pull-request anecdotes.

#### N4. JSON schema testing

§3 noted "stability commitment for JSON output. Worth committing to before the first non-MVP release." That release happened (we're at 2026.4.28). The commitment exists in `AGENTS.md` ("JSON schema is stable across patch releases within a CalVer minor"). What's missing is the *test* that enforces it.

Add `tests/json_schema.rs`:

- Snapshot the JSON output of `cr check` on a frozen fixture corpus.
- Fail the build if the field names or types change without a corresponding bump.
- Require a comment in the test pointing at the schema-version bump in `AGENTS.md` to update the snapshot.

Same approach for the `casual-review/finding/1` and `casual-review/comment/1` schemas — snapshot test the JSON, refuse silent breakage.

#### N5. Editor-extension marketplace publishing

All three extensions build in CI; none are published to their respective marketplaces:

- **VS Code Marketplace** + **OpenVSX** — needs a `vsce publish` step in the release workflow, gated on a `VSCE_PAT`/`OVSX_PAT` secret. Cost: ~4 hours including secret setup.
- **JetBrains Marketplace** — `gradle publishPlugin` step + `JETBRAINS_HUB_TOKEN`. Cost: similar.
- **Zed extensions registry** — PR to `zed-industries/extensions`. Manual once; subsequent updates are tag-driven on the casual-review side.

Each is gated by tag-driven release in `release.yml`. Until publishing lands, users discover extensions via "build from source," which is a real adoption tax.

#### N6. MSRV enforcement

§1 said "pin to current stable minus one. Document it." `Cargo.toml` has `rust-version = "1.80"`. CI runs `dtolnay/rust-toolchain@stable` on every job — so MSRV is documented but not actually tested. Add an MSRV job pinned to `1.80` (or whatever the declared MSRV is) running `cargo build && cargo test`. When the declared MSRV bumps, the job updates with the same PR.

#### N7. Test coverage hygiene

Snapshot tests cover ~52 unit cases (one fixture per rule) plus integration tests for Phases 3 and 4. Two gaps:

1. **Rule × language matrix.** Universal rules should have a snapshot fixture per language. `commented-code` has Rust + TS but not Python or Java. Catalogue the matrix; fill the holes.
2. **`cr explain` regression.** Each rule's `explain()` text is part of the user contract. Add a snapshot test over the full output of `cr explain` (no rule arg) and one per rule for `cr explain <id>`. Catches accidental edits to the help copy.

#### N8. Windows behaviour audit

CI runs the test suite on Windows, but no one reviews the *output* on Windows. ariadne's Unicode rendering, line-ending handling in `trailing-whitespace`, and path display in JSON all benefit from a manual Windows pass. Land snapshot fixtures with `\r\n` line endings to lock the behaviour in.

#### N9. Documentation: rule explain audit

`cr explain <rule-id>` is the canonical rule documentation surface. Each rule's `explain()` should answer three questions: what it catches, why it matters, how to fix. Audit the 15 existing rules' explain text; rewrite any that bury one of the three. (Quick spot-check: `commented-code` answers all three; `parse-error` is terse — could explain why a parse error becomes a diagnostic at all.)

#### N10. Release-notes story

`make release` tags + builds. There's no `CHANGELOG.md` or auto-generated release notes. For a CalVer-versioned tool, "what changed in this release" is the only orientation a user gets between `2026.4.28` and `2026.5.3`. Adopt one of:

- Hand-written `CHANGELOG.md` updated as part of `make release`.
- Auto-generated notes via `git cliff` from conventional commit messages, regenerated each release.

Lean toward `git cliff` if commit hygiene is good (current `git log` is mixed). Otherwise hand-written. Either way, this is a near-term blocker on growing the user base.

### 12.3 Sequencing

The functional and non-functional threads run in parallel, but there's a natural ordering within each:

| Order | Functional | Non-functional |
|---|---|---|
| 1 | F1 — rule precision | N1 — self-eval gate |
| 2 | F2 — `cr-allow` directives | N4 — JSON schema test |
| 3 | F3 — `cr fix` (start with trivial rules) | N3 — CI bench gate |
| 4 | F4 — `--format agent` | N2 — performance closure |
| 5 | F9 — incremental new rules | N5 — marketplace publishing |
| 6 | F8 — `unused-public` (cross-file spike) | N7, N8, N9, N10 — coverage / docs / Windows / changelog |
| 7 | F5 — smart re-anchor (gated on metrics) | N6 — MSRV gate |
| 8 | F7 — Go (or whichever language gets demand) | — |
| 9 | F6 — forge bridge (gated on demand) | — |
| 10 | F10 — Neovim (gated on demand) | — |

Items 1–4 are the immediate priority. Items 5–10 ride on demand, profiling results, or completion of earlier work.

### Next decisions for section 12

- **Self-eval gate as blocking CI.** Recommend yes once F1 + F2 land; the project's credibility rests on its own diagnostics being signal.
- **`cr-allow` syntax: `cr-allow:` vs `cr:allow`.** The colon-after form is closer to `clippy::allow` mental model; the no-colon form scans easier. Lean colon-after for consistency with `cr-fetch` etc.
- **Bench gate tolerance.** 10% (the §8 figure) on shared runners is tight enough to false-positive often; 15% is permissive but reduces churn. Recommend 15% on Ubuntu-latest, tighten to 10% if a self-hosted runner becomes available.
- **Whether `cr fix` writes only to changed files.** Yes by default (matches the rest of the diff-aware UX); `--all` extends the scope.

---

## Phasing summary

| Phase | Scope | Status | Exit criteria |
|---|---|---|---|
| **Phase 1 — MVP** | Single-crate layout, diagnostic types, ariadne renderer, two languages (Rust + Python), three rules, diff-aware default, JSON output, exit codes | ✅ shipped (overshot: 15 rules, 5 languages) | `cr check` on a fixture matches snapshot, runs in <200 ms, CI integration via JSON |
| **Phase 2 — CI-grade** | SARIF output, `--format github`, more languages, `casual-review.toml` config, benchmark suite, `--all` mode | ✅ shipped (Go grammar still pending; CI bench-regression gate still pending) | Adopted in at least one real repo's CI; SARIF appears in GitHub code scanning UI |
| **Phase 3 — Substrate** | `cr publish`/`show`/`fetch`/`push` against `refs/notes/casual-review`, JSON schema v1 frozen, threaded ack/dismiss | ✅ shipped | Findings round-trip through clone+push+pull |
| **Phase 4 — Collaboration** | Ref split, `casual-review/comment/1` schema, `cr comment` subcommands, anchoring/staleness, VS Code + JetBrains + Zed extensions | ✅ shipped except 4.2 smart re-anchor and 4.4 forge bridge | Two developers leave and reply to comments end-to-end from an editor via the git remote |
| **Phase 5 — Hardening & growth** | Rule precision, suppression UX, `cr fix`, performance, schema testing, more languages, smart re-anchor, forge bridge, agent format, marketplace publishing | ⏳ active — see §12 | Self-eval clean on own tree; bench targets met; one external repo using `cr` in CI; extensions on official marketplaces |
| **Phase 6+ — Driven by demand** | WASM rules, external-process rules, LSP server, Neovim plugin, cross-file analysis | Not started | Driven by real demand, not by this plan |

---

## Immediate next steps

The MVP-bring-up checklist below is preserved as a record. For active work, see §12.

1. ~~Decide on ariadne vs codespan-reporting.~~ Done — ariadne.
2. ~~Decide on the initial post-MVP language set.~~ TypeScript + TSX + Java shipped; Go remains.
3. ~~Initialize the workspace, single-crate layout.~~ Done.
4. ~~Implement diagnostic types end-to-end and snapshot-test the human renderer.~~ Done.
5. ~~Wire `cr-git` for the working-tree diff case.~~ Done; staged + repo-wide also shipped.
6. ~~Add `tree-sitter-rust` and the MVP rules.~~ Done; 15 rules across 5 languages.

The current top of the queue lives in §12. The single sharpest item: **fix `commented-code` false positives on doc comments** so `make selfcheck` is signal, not noise.
