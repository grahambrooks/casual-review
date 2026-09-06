# Casual Review — Zed Extension

Exposes [casual-review](../..)'s git-stored review comments to Zed's **Agent**
through an MCP server.

## How it works

Zed removed extension-provided slash commands along with text threads
([zed#53760](https://github.com/zed-industries/zed/issues/53760)). The modern
integration path is the **Model Context Protocol**: this extension registers an
MCP server that Zed's Agent connects to. The server is `cr mcp` — the same `cr`
binary in stdio MCP mode — which exposes the review operations as tools:

| Tool | Purpose |
|---|---|
| `list_comments` | Read open threads on a commit (JSON payload). |
| `add_comment` | Anchor a comment to lines, a file, or the commit. |
| `reply_comment` | Reply to a thread, inheriting its anchor. |
| `resolve_comment` | Mark a thread resolved (append-only). |
| `reanchor_comment` | Move a stale comment to a new line range. |
| `sync_comments` | `fetch` / `push` / `sync` comments with a remote. |

So you can ask Zed's Agent to "review this function and leave comments," or
"show open review threads on this file and resolve the ones I've addressed,"
and it drives `cr` directly — no copy-pasting shell commands.

## Requirements

`cr` must be installed and resolvable when Zed launches the server. By default
the extension runs `cr` from `PATH`. If `cr` lives elsewhere (e.g. Zed was
launched from Finder without your shell `PATH`), override the command in your
Zed `settings.json`:

```jsonc
"context_servers": {
  "casual-review": {
    "command": { "path": "/absolute/path/to/cr", "args": ["mcp"] }
  }
}
```

## Build

```sh
rustup target add wasm32-wasip1
cd extensions/zed
cargo build --release --target wasm32-wasip1
```

## Install (dev mode)

In Zed: **Extensions → Install Dev Extension** and pick `extensions/zed/`. Zed
compiles the extension itself. Then open the **Agent** panel — the
`casual-review` tools become available to the Agent. Confirm it's connected
under **Settings → Context Servers**.

## Other editors

For inline gutter decorations and reply boxes, use the editor-native clients:

- VS Code → `extensions/vscode/`
- JetBrains → `extensions/jetbrains/`
