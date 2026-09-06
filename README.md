# casual-review

Code reviews (human or agentic) are key for group/team learning. Current code review tools provide comments based on change - pull request reviews. This strategy assumes that each review is actioned on that code and around the changes are also considered as part of the review which is optimistic. Traditional code review tools are now very rare.

## Key differences

* `casual-review` is a tool that supports code review feedback on the entire file/module/project, not just the changes.
* Stores review comments and conversations inside the git repository.
* Review comments are pushed to the SCM alongside the code changes, so that they are versioned and can be reviewed in context.
* Supports both human and AI agents as reviewers, with a focus on making the tool useful for LLM agents (Claude Code, Cursor, etc.) as well as humans.
* Review comments stay associated with the code until addressed.

## How it works

A review comment is anchored to a **line range**, a **whole file**, or a **commit**, and stored as a structured record on a dedicated git-notes ref (`refs/notes/casual-review/discuss`). Because the data lives in git — not a third-party server — it travels with the repo: `cr push`/`cr fetch` move comments between clones exactly like code.

Threads are append-only. A reply, a resolution, and a re-anchor are each a new record linked to its parent, so history is never rewritten and the full conversation is auditable. Each comment carries a content hash of the code it points at, so the CLI and extensions can flag a comment as **stale** when the underlying code changes — keeping feedback associated with the code until it's actually addressed.

## Plugins

* VS Code extension: [casual-review-vscode](extensions/vscode)
* JetBrains IDEs: [casual-review-jetbrains](extensions/jetbrains)
* Zed extension: [casual-review-zed](extensions/zed)

The VS Code and JetBrains extensions are thin UIs over the `cr` CLI: they add comments from a selection, show threads in a side panel, surface stale anchors, and run sync. The Zed extension integrates differently — it registers `cr mcp` as an **MCP server** so Zed's Agent can read and write review comments directly (see [MCP server](#mcp-server-for-ai-agents)). In all cases `cr` must be on `PATH` (or set the extension's binary-path setting).

## CLI Install

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

Supported binary platforms: Apple Silicon (`aarch64-apple-darwin`), Linux x86_64 (`x86_64-unknown-linux-gnu`), Linux ARM64 (`aarch64-unknown-linux-gnu`), Windows x86_64 (`x86_64-pc-windows-msvc`). Intel Mac is not distributed as a binary — `cargo install` (below) handles it.

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

`casual-review` uses **CalVer** (`YYYY.M.D`) — the version is literally year, month (1-12), and day-of-month (1-31). Cadence is opportunistic, not scheduled; same-day re-releases use the following day's number.

## Usage

```sh
# Add a comment anchored to lines 42–44 of a file (opens $EDITOR for the body)
cr comment add src/auth.rs --lines 42:44

# ...or pass the body inline
cr comment add src/auth.rs --lines 42:44 -m "this branch never handles the expired-token case"

# Comment on a whole file, or on the commit as a whole
cr comment add src/auth.rs --file-level -m "module mixes transport and policy concerns"
cr comment add --commit-level -m "good direction; let's split this into two commits"

# List the open threads on HEAD (human-readable; [stale] marks drifted anchors)
cr comment list

# Include threads inherited from ancestor commits, and emit JSON for an agent
cr comment list --include-ancestors --format json

# Continue a conversation, resolve it, or move a stale anchor
cr comment reply  CRC-1a2b3c4d5e6f -m "fixed in the follow-up commit"
cr comment resolve CRC-1a2b3c4d5e6f -m "addressed"
cr comment reanchor CRC-1a2b3c4d5e6f --lines 51:53

# Share with the team — comments ride git, not a server
cr fetch          # pull the discuss ref from origin
cr push           # publish your comments
cr sync           # fetch + push in one step
```

Comments target `HEAD` by default; pass `--commit <rev>` to read or write on another commit. `cr comment add` and `cr comment reply` print the new comment ID to stdout (and a status line to stderr), so they compose in scripts.

Exit codes: `0` success, `2` tool failure (not a git repo, unknown commit, empty body, etc.).

## MCP server (for AI agents)

`cr mcp` runs an [MCP](https://modelcontextprotocol.io) server over stdio, exposing the review operations as tools — `list_comments`, `add_comment`, `reply_comment`, `resolve_comment`, `reanchor_comment`, and `sync_comments`. Any MCP host (Zed's Agent, Claude, Cursor, …) can then read and write review comments directly instead of shelling out.

The Zed extension wires this up automatically. For other hosts, register `cr mcp` as an MCP server — for example in a Claude/Cursor MCP config:

```jsonc
{
  "mcpServers": {
    "casual-review": { "command": "cr", "args": ["mcp"] }
  }
}
```

The server operates on the repo in its working directory and uses the same `casual-review/comment/1` schema as the CLI.

## Using cr with an AI agent

`cr` is designed to be useful to LLM agents (Claude Code, Cursor, etc.) as well as humans — an agent reviewer leaves comments in exactly the same store a human does, and a human can reply to them. The workflow, JSON schema, and anchoring semantics are documented in [AGENTS.md](AGENTS.md). Claude Code users will find a brief orientation in [CLAUDE.md](CLAUDE.md).

In short: an agent calls `cr comment list --format json` to read existing review state, `cr comment add ... -m "..."` to leave anchored feedback, and `cr comment reply/resolve` to participate in threads. The `casual-review/comment/1` JSON schema is stable across patch releases within a CalVer minor.

## Tech

- **Rust** for a fast, single-binary CLI with no runtime dependencies.
- **git notes** (`refs/notes/casual-review/discuss`) as the storage substrate — versioned, syncable, and rewrite-free.
- **SHA-256** content hashing for stable comment IDs and staleness detection.
- **serde / serde_json** for the on-disk comment schema.

When the working directory is not a git repository, the CLI falls back to a local `.cr-comments/` directory so comments are never lost; the git-notes path is used whenever it's available.

## Layout

Single binary crate, subsystems as directory modules under `src/`:

```
src/comments.rs      Comment / Anchor / Author / CommentsPayload schema + IDs
src/review.rs        Core comment operations (add/list/reply/resolve/reanchor), front-end agnostic
src/git_comments.rs  Persistence on refs/notes/casual-review/discuss (+ file fallback)
src/notes_io.rs      Schema-agnostic git-notes read/write/fetch/push primitives
src/mcp.rs           stdio MCP server (cr mcp) exposing review.rs as tools
src/cli.rs           clap definitions
src/main.rs          CLI command handlers (thin wrappers over review.rs)
```

## Project files

- [AGENTS.md](AGENTS.md) — workflow, JSON schema, and anchoring semantics for AI-agent reviewers
- [context.md](context.md) — design notes on git as a substrate for review/agent annotations

## Releases

Releases are tag-driven. Pushing a tag matching `v*.*.*` (e.g., `v2026.5.0`) triggers `.github/workflows/release.yml`, which:

1. Verifies the tag matches `Cargo.toml`'s version.
2. Builds release binaries for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`.
3. Uploads each archive (with sha256 sidecar) to the GitHub Release.
4. Updates `Formula/casual-review.rb` with the new version + per-platform SHA256s and commits the change back to `main`.

To cut a release:

```sh
make release                  # tag today's CalVer (YYYY.M.D)
make release VERSION=2026.5.3 # or bump to an explicit version, then tag
git push origin main --follow-tags
```

## CI

`.github/workflows/ci.yml` runs on every push to `main` and every pull request: `cargo build`/`test` on Linux, macOS, and Windows; `cargo fmt --check`; `cargo clippy -D warnings`.

## License

Dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
