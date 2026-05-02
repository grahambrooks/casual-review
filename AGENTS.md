# AGENTS.md

Guidance for AI agents using `casual-review` to find and resolve issues in a codebase.

`cr` is a CLI that produces structured diagnostics. This document describes the workflow, output schema, and rule semantics an agent needs to use it effectively.

1. Don’t assume. Don’t hide confusion. Surface tradeoffs.
2. Minimum code that solves the problem. Nothing speculative.
3. Touch only what you must. Clean up only your own mess.
4. Define success criteria. Loop until verified.

## When to reach for cr

- The user asks you to **review** code, a PR, or recent changes.
- The user asks you to **fix issues** in a repo and you don't know where to start.
- You're about to make changes and want to know whether existing code already has lint debt to avoid amplifying.

`cr` is specifically diff-aware by default — it won't drown you in findings on unchanged code unless you ask it to.

## The five-step workflow

1. **Run cr** in the most useful mode for the task (see below).
2. **Parse the JSON output**, one diagnostic per line.
3. **Prioritize by severity**: `error` first, then `warning`, then `note`.
4. **Decide per finding**: fix, ack-and-move-on, or skip with explanation.
5. **Re-run cr after changes** to verify your fixes didn't introduce new issues.

## Choosing the mode

| Goal | Command |
|---|---|
| Review the current PR / working-tree changes | `cr check --format json` |
| Review staged changes only | `cr check --staged --format json` |
| Review code being added or changed (with full file context, not just diff) | `cr check --all --format json` |
| First-pass evaluation of an unfamiliar codebase | `cr check --repo --verbose --format json` |
| Lint a specific file you're about to edit | `cr check path/to/file.rs --format json` |

The default mode (`cr check` with no flags) is the right starting point most of the time: it lints exactly what's in the working-tree diff against `HEAD`. Reach for `--repo` only when there's no useful diff (clean tree, brand-new file, exploring an unfamiliar repo).

## Reading the output

Each line of `cr check --format json` is one JSON diagnostic with this shape:

```json
{
  "code": "cognitive-complexity",
  "severity": "warning",
  "message": "function `extract_ts` has cognitive complexity 33 (threshold: 15)",
  "primary": {
    "file": "src/rules/api_surface_change.rs",
    "byte_range": {"start": 7056, "end": 7136},
    "line_start": 204,
    "col_start": 1,
    "line_end": 205,
    "col_end": 10
  },
  "labels": [],
  "notes": ["score grows with nesting depth; flat code with the same number of branches scores much lower"],
  "helps": ["extract helpers, return early, or invert conditions to reduce nesting"],
  "suggestions": []
}
```

Field semantics:

- **`code`** — stable rule id. Use this to look up rule semantics with `cr explain <code>`.
- **`severity`** — `error` (exit 1), `warning` (exit 0 but worth fixing), `note`/`help` (informational).
- **`message`** — one-line summary of what's wrong.
- **`primary`** — where to point the fix. `line_start`/`col_start` are 1-based and editor-friendly.
- **`labels`** — secondary spans with their own messages (e.g., a related location).
- **`notes`** / **`helps`** — additional context. `helps` is usually the actionable hint.
- **`suggestions`** — structured fix proposals (currently rare; will grow over time).

## Rule semantics

Run `cr explain` (no argument) to list all rules with one-line summaries.
Run `cr explain <rule-id>` for the full documentation including what it catches, why it matters, and how to fix it.

The 15 rules grouped by character:

**Universal high-signal** (almost always worth surfacing in a review):
- `parse-error` — file doesn't parse (Error severity). Fix the syntax.
- `cognitive-complexity` — function is hard to read. Score > 15 = candidate for splitting.
- `empty-catch` — silent error swallowing. Real bugs hide here.
- `assertion-free-test` — test that can't fail meaningfully. Add an assertion or delete.
- `hardcoded-secret` — committed API key / token (Error severity). Rotate, then remove.

**Universal medium-signal** (review nudges):
- `large-function` — body > 40 lines. Heuristic; watch for false positives in long match statements.
- `debug-print` — `println!`/`console.log` etc. that probably shouldn't ship.
- `disabled-test` — `#[ignore]`/`it.skip` etc. Why is it disabled?
- `todo-marker` — TODO/FIXME/XXX. Should this be a tracked issue?
- `trailing-whitespace` — cosmetic.

**Language-specific**:
- `unwrap-used` (Rust) — `.unwrap()`/`.expect()`. Test code is fine; production usually isn't.
- `any-type` (TS/TSX) — explicit `any`. Use `unknown` or a real type.
- `ts-escape-hatch` (TS/TSX) — `@ts-ignore`/`@ts-nocheck`/non-null `!`.
- `bare-except` (Python) — `except:` without a type. Catches `KeyboardInterrupt` too.

**Diff-aware** (only fires when there's a HEAD blob to compare against):
- `api-surface-change` — public symbols added/removed in the diff. Note severity — heads-up for reviewers, not a problem.

## Decision rubric for fixes

For each finding the agent retrieves, classify into one of:

1. **Fix it** — clearly correct, low-risk, fits the user's stated task.
2. **Surface it** — explain to the user what fired and let them decide. Use this when the fix is invasive or the rule has a high false-positive rate in this context.
3. **Skip it** — when the finding is in code the user didn't ask you to touch, or in code that's clearly out of scope (vendored, generated, fixtures).

Default to **surface, not silently skip**. If you're unsure whether a finding matters, tell the user it fired and ask.

## Common workflows

**Workflow: "review my recent changes"**

```sh
cr check --format json | jq -c '.'
```

Walk the findings, group by severity, present to the user. For errors, propose fixes inline. For warnings, summarise and ask which to address.

**Workflow: "fix all high-severity issues in this repo"**

```sh
cr check --repo --format json | jq -c 'select(.severity == "error")'
```

Iterate over errors. For each, read the file at the reported line, propose a fix, apply if low-risk.

**Workflow: "should I add this code or refactor first?"**

```sh
cr check --repo path/to/area --format json | jq -c '.'
```

Look at existing complexity, unwrap-usage, etc. in the area. If the area is already noisy, refactoring before adding may be cheaper than amplifying the noise.

## Performance and limits

- Throughput: ~280k LOC/sec single-thread, ~550k LOC/sec on 8 cores. A 500k-LOC repo finishes in ~2 seconds.
- Cold-startup: ~6ms (negligible).
- Languages: Rust, Python, TypeScript, TSX, Java. Other files are silently skipped.

## What cr deliberately doesn't do

- **No fix application.** `cr` reports findings; an agent (or human) decides what to fix and how. A future `cr fix` is on the roadmap but isn't built.
- **No config file (yet).** All rules fire with built-in thresholds. Per-rule disable / per-path suppression is the next operational priority — until then, file-list filtering (`cr check src/ tests/`) is the only suppression mechanism.
- **No cross-file analysis.** Each file is parsed independently. Dead-code, unused-export, and import-graph analyses are out of scope for v1.

## Stability commitments

- **JSON schema (`--format json`)** is stable across patch releases within a CalVer minor (`YYYY.M.*`). Major or minor bumps may add fields; existing field names and types won't change without a version bump.
- **Rule ids** are stable. New rules add new ids; rules don't get renamed.
- **Exit codes** — `0` clean, `1` errors found, `2` tool failure (config error, can't read repo, etc.).

## Reporting issues

If a rule produces a false positive on code you believe is correct, the most useful bug report includes:

1. The exact JSON diagnostic.
2. A minimal reproducible code snippet.
3. What you expected the rule to do.

Open at <https://github.com/grahambrooks/casual-review/issues>.
