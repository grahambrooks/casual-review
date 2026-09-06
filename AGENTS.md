# AGENTS.md

Guidance for AI agents using `casual-review` to leave, read, and resolve code-review feedback that lives **inside the git repository**.

`cr` is a CLI that stores review comments and conversations as structured records on a git-notes ref (`refs/notes/casual-review/discuss`). An agent reviewer writes to the same store a human does, and a human can reply to an agent's comment (and vice versa). This document describes the workflow, the JSON schema, and the anchoring semantics an agent needs to use it effectively.

**If your host speaks MCP** (Zed's Agent, Claude, Cursor, …), register `cr mcp` as an MCP server and use the tools `list_comments` / `add_comment` / `reply_comment` / `resolve_comment` / `reanchor_comment` / `sync_comments` instead of shelling out — they map one-to-one to the CLI below and return the same `casual-review/comment/1` payloads. The rest of this document (workflow, schema, anchoring) applies identically.

1. Don't assume. Don't hide confusion. Surface tradeoffs.
2. Review the whole unit you were asked about — file, module, or commit — not only the diff.
3. Anchor every comment precisely, and say why it matters and what to do.
4. Leave the conversation in git; don't dump findings into chat and lose them.

## When to reach for cr

- The user asks you to **review** a file, module, commit, or PR and have the feedback persist.
- You're collaborating with humans (or other agents) who will read and act on review comments later, possibly in their editor via the extensions.
- You want feedback to stay **associated with the code until addressed** — `cr` tracks staleness as the code drifts.

Unlike a PR review tool, `cr` is not limited to changed lines: comment on any line range, any whole file, or the commit as a whole.

## The workflow

1. **Read existing state** so you don't duplicate feedback: `cr comment list --format json` (add `--include-ancestors` to pull threads written on earlier commits).
2. **Review the unit** the user pointed you at.
3. **Leave anchored comments**: `cr comment add <file> --lines A:B -m "..."` (or `--file-level` / `--commit-level`).
4. **Participate in threads**: `cr comment reply <id>` and `cr comment resolve <id>` when something is addressed.
5. **Sync** if the user wants the team to see it: `cr push` (or `cr sync`).

## Commands

| Goal | Command |
|---|---|
| Read open threads on HEAD as JSON | `cr comment list --format json` |
| Read threads, including ones inherited from ancestors | `cr comment list --include-ancestors --format json` |
| Read threads on a specific commit | `cr comment list --commit <rev> --format json` |
| Comment on a line range | `cr comment add <file> --lines A:B -m "..."` |
| Comment on a whole file | `cr comment add <file> --file-level -m "..."` |
| Comment on the commit as a whole | `cr comment add --commit-level -m "..."` |
| Reply to a thread (inherits the parent's anchor) | `cr comment reply <id> -m "..."` |
| Mark a thread resolved | `cr comment resolve <id> -m "..."` |
| Move a stale comment to a new line range | `cr comment reanchor <id> --lines A:B` |
| Pull / publish / sync comments with a remote | `cr fetch` / `cr push` / `cr sync` |

`add` and `reply` print the new comment ID to **stdout** (status text goes to stderr), so capture stdout to thread further. Always pass `-m` so you never trigger the interactive `$EDITOR` fallback — a missing body aborts.

## Reading the output

`cr comment list --format json` prints a single `casual-review/comment/1` payload:

```json
{
  "schema": "casual-review/comment/1",
  "tool": "casual-review",
  "tool_version": "2026.5.2",
  "commit": "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c",
  "comments": [
    {
      "id": "CRC-1a2b3c4d5e6f",
      "author": { "name": "Reviewer Bot", "email": "bot@example.com" },
      "created_at": "2026-06-27T14:03:55+00:00",
      "anchor": {
        "file": "src/auth.rs",
        "line_range": [42, 44],
        "byte_range": [1280, 1361],
        "anchor_text_sha": "b1946ac92492d2347c6235b4d2611184…"
      },
      "body": "this branch never handles the expired-token case",
      "resolved": false
    }
  ]
}
```

Field semantics:

- **`id`** — stable `CRC-<12 hex>` hash of author + timestamp + anchor + body. Use it as the target for `reply` / `resolve` / `reanchor`.
- **`author`** — sourced from `git config user.{name,email}`. Set those before commenting or `add` errors (no anonymous comments).
- **`anchor`** — where the comment points (see below). `file` absent ⇒ commit-level; `line_range` of `[0, 0]` with a `file` ⇒ file-level; otherwise a line/byte range.
- **`anchor_text_sha`** — SHA-256 of the anchored bytes at creation time. If a fresh hash of the current code differs, the comment is **stale** (human output marks it `[stale]`).
- **`parent`** — present on replies, resolutions, and re-anchors; it's the ID of the record they extend. Top-level comments omit it.
- **`resolved`** — `true` only on the resolution record itself. A thread is resolved when any record carries `resolved: true` for its root; `cr comment list` hides resolved threads unless you pass `--include-resolved`.
- **`origin_commit`** — only set by `--include-ancestors`, naming the commit a projected comment was originally written on. Never persisted.

## Anchoring semantics

- **Line-level** (`--lines A:B`) is the default and most useful. Lines are 1-based and inclusive.
- **File-level** (`--file-level`) attaches to the file as a whole — good for module-shape or organization feedback.
- **Commit-level** (`--commit-level`) attaches to the commit with no file anchor — good for review summaries or cross-cutting notes.
- **Staleness**: anchors store a content hash, not just coordinates. When code shifts, a comment goes stale rather than silently pointing at the wrong lines. If a stale comment is still relevant, `cr comment reanchor <id> --lines A:B` moves it; otherwise reply/resolve it.
- **Append-only**: replies, resolutions, and re-anchors are new records linked by `parent`. Nothing is mutated or deleted, so the thread is fully auditable.

## Decision rubric for leaving a comment

For each issue you find, classify it:

1. **Comment it** — anchor a comment with a clear rationale and a concrete suggestion. Prefer line-level; fall back to file/commit-level when the concern is structural.
2. **Reply, don't restate** — if a relevant thread already exists (check with `cr comment list` first), reply to it instead of opening a duplicate.
3. **Resolve** — when you've confirmed something raised earlier is addressed, `resolve` it with a one-line note.

Default to **anchored and specific**. A comment that doesn't say what to change is noise.

## Sync model

Comments live on `refs/notes/casual-review/discuss`. They are local until pushed.

- `cr fetch [remote]` — pull the discuss ref (a missing remote ref is a no-op, not an error).
- `cr push [remote]` — publish your comments.
- `cr sync [remote]` — fetch then push. `remote` defaults to `origin`.

Only push when the user wants the team to see the feedback; local review iterations don't need it.

## What cr deliberately doesn't do

- **No built-in lint rules.** `cr` stores review *judgment* (yours and humans'), not mechanical lint output. Run a linter separately; record what's worth a conversation as a comment.
- **No history rewriting.** Edits are new append-only records, never mutations.
- **No server.** The repo is the database; sync is plain `git fetch`/`push` of one notes ref.

## Stability commitments

- **JSON schema (`casual-review/comment/1`)** is stable across patch releases within a CalVer minor (`YYYY.M.*`). Minor/major bumps may add fields; existing field names and types won't change without a version bump.
- **Comment IDs** are content-stable: the same author, timestamp, anchor, and body always hash to the same `CRC-…` id, on any machine.
- **Exit codes** — `0` success, `2` tool failure (not a git repo, unknown commit, unset `user.name`/`user.email`, empty body, etc.).

## Reporting issues

Open at <https://github.com/grahambrooks/casual-review/issues>. A useful report includes the exact `cr` command, the JSON payload involved, and what you expected.
