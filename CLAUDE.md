# CLAUDE.md

This file is a Claude-Code-friendly entry point. The full agent guidance lives in [AGENTS.md](AGENTS.md) — start there.

## Quick orientation

`casual-review` stores code-review comments and conversations **inside the git repository**. The binary is `cr`. Comments are anchored to a line range, a whole file, or a commit, and persisted as structured records on a git-notes ref (`refs/notes/casual-review/discuss`) — so review feedback is versioned, syncable, and travels with the code. Both humans and AI agents are first-class reviewers writing to the same store.

## When you're using cr in someone's repo

See [AGENTS.md](AGENTS.md). The workflow is:

1. `cr comment list --format json` — read existing review state (add `--include-ancestors`).
2. Review the file/module/commit you were asked about (not just the diff).
3. `cr comment add <file> --lines A:B -m "..."` — leave anchored feedback.
4. `cr comment reply <id>` / `cr comment resolve <id>` — participate in threads.
5. `cr push` (or `cr sync`) when the team should see it.

Always pass `-m` to avoid the interactive `$EDITOR` fallback. `add`/`reply` print the new comment ID to stdout.

## When you're contributing TO casual-review (this repo)

Conventions and operations:

- **Build / test**: `make build`, `make test`. Or `cargo build` / `cargo test` directly.
- **Lint**: `make lint` (= `fmt-check` + `clippy -D warnings`). CI enforces both.
- **Run the extensions**: `make ext-run` lists the per-editor dev targets (they shell out to `cr`, so install it first).

## Architecture in one paragraph

Single binary crate. `src/comments.rs` holds the on-disk schema — `Comment`, `Anchor`, `Author`, `CommentsPayload` — plus content-hash comment IDs and staleness hashing (`sha256_hex`). `src/notes_io.rs` is the schema-agnostic git-notes layer: read/write/fetch/push raw bytes for any `refs/notes/<ref>`, shelling out to `git`. `src/git_comments.rs` composes the two — it persists `CommentsPayload` JSON on `refs/notes/casual-review/discuss`, with a `.cr-comments/` file fallback when the cwd isn't a git repo. `src/review.rs` is the **front-end-agnostic core**: `add`/`list`/`reply`/`resolve`/`reanchor` operating on a repo path and returning `Comment` records (line↔byte range resolution, anchor building, ancestor projection, staleness all live here). Two front ends call it: `src/main.rs` (clap CLI handlers from `src/cli.rs`, which only parse args and print) and `src/mcp.rs` (a stdio MCP server, `cr mcp`, exposing the same operations as tools for AI agent hosts like Zed's Agent).

## The data model

- **Anchor**: `file = None` ⇒ commit-level; `file` set with `line_range = (0,0)` ⇒ file-level; otherwise a line+byte range. `anchor_text_sha` is the SHA-256 of the anchored bytes at creation, used to detect drift.
- **Append-only threads**: replies, resolutions, and re-anchors are *new* `Comment` records linked by `parent`. Nothing is mutated; a thread's state is derived by walking its records.
- **Resolution**: a record with `resolved: true` whose `parent` is the thread root. `cr comment list` hides resolved threads unless `--include-resolved`.
- **Ancestor projection**: `--include-ancestors` pulls comments from ancestor commits onto the target, tagging each with `origin_commit` (never persisted) and recomputing staleness against the working tree.

## When you change the comment schema

1. Edit `src/comments.rs`. Keep the schema string (`casual-review/comment/1`) and bump it only on a breaking change.
2. Preserve serde back-compat: new fields need `#[serde(default)]`; fields not meant to persist need `skip_serializing_if`.
3. Add/extend tests in `tests/phase4_comments.rs` (storage, threading, IDs) and `tests/phase4_anchoring.rs` (line↔byte ranges, staleness, ancestor projection).
4. `make lint && make test` before committing.

## Things to know

- The user prefers a single binary crate with directory modules over Cargo workspaces (this is in `memory/feedback_crate_layout.md`).
- All git operations shell out to the `git` binary (via `std::process::Command`), not a library — keep that consistent.
- `notes_io::migrate_legacy_findings_ref` exists to move the old `refs/notes/casual-review` ref aside (git refuses both `casual-review` and `casual-review/<sub>` at once). Leave it in the read/write path.
- The user prefers terse responses; don't summarise diffs they can read themselves.
