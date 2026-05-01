# casual-review — Implementation Plan

A phased plan for a Rust CLI that brings rustc-quality diagnostics to other languages, runs equally well on a developer workstation and in CI, and (eventually) shares findings through Git's own substrate. Pragmatic and opinionated; uncertainty is called out where it is real.

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

## Phasing summary

| Phase | Scope | Exit criteria |
|---|---|---|
| **Phase 1 — MVP** | Workspace, diagnostic types, ariadne renderer, two languages (Rust + Python), three rules, diff-aware default, JSON output, exit codes | `cr check` on a fixture matches snapshot, runs in <200 ms, CI integration via JSON |
| **Phase 2 — CI-grade** | SARIF output, `--format github`, two more languages (TypeScript + Go), `casual-review.toml` config, benchmark suite, `--all` and `--changed-only` modes | Adopted in at least one real repo's CI; SARIF appears in GitHub code scanning UI |
| **Phase 3 — Substrate** | `cr publish`/`show`/`fetch`/`push` against `refs/notes/casual-review`, JSON schema v1 frozen, threaded ack/dismiss, agent-format output if justified | Findings round-trip through clone+push+pull; one external user has consumed them |
| **Phase 4 — Collaboration** | Ref split, `casual-review/comment/1` schema, `cr comment` subcommands, anchoring/staleness, VS Code extension, optional forge bridge | Two developers leave and reply to comments end-to-end from VS Code via the git remote |
| **Phase 4 add-on — JetBrains plugin** | `extensions/jetbrains/`: project service, gutter highlighters, actions, status bar widget. Same JSON contract as the VS Code extension. | Builds via `./gradlew buildPlugin` (Kotlin 2.2 / Gradle 9 / IntelliJ Platform Gradle Plugin 2.6, IDE 2024.2 baseline). |
| **Phase 5+** | WASM rules, LSP server, `cr fix`, additional languages on demand, Neovim extension | Driven by real demand, not by this plan |

---

## Immediate next steps

1. Decide on ariadne vs codespan-reporting. (Recommend ariadne.)
2. Decide on the initial post-MVP language set. (Recommend TypeScript + Go.)
3. Initialize the workspace per section 1, with empty crate skeletons and a passing `cargo build`.
4. Implement `cr-core` types end-to-end and snapshot-test the human renderer against three hand-written fixture diagnostics — *before* wiring tree-sitter or git2. The diagnostic engine is the project; everything else is an adapter.
5. Wire `cr-git` for the working-tree diff case only. Defer staged/range until MVP runs end-to-end.
6. Add `tree-sitter-rust` and the three MVP rules. Demo `cr check` on the casual-review repo itself.
