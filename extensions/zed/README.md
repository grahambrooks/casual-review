# Casual Review — Zed Extension

Slash commands for the [casual-review](../..) CLI inside Zed's Assistant
panel.

## Why slash commands and not gutter decorations?

Zed extensions run in a WASM sandbox without arbitrary subprocess access.
The extension cannot spawn `cr` directly the way the VS Code or JetBrains
extensions do, so it cannot draw live gutter markers or open inline reply
boxes — those features require running `cr comment list` and reacting to
its JSON output.

What this extension *can* do is help you (and Zed's AI assistant) compose
correct `cr` invocations. The Assistant pairs especially well with this
pattern: ask "review this function" and have it propose
`cr comment add ...` calls you can paste into a terminal.

For full editor integration, install the matching client:
- VS Code → `extensions/vscode/`
- JetBrains → `extensions/jetbrains/`

## Build

```sh
rustup target add wasm32-wasip1
cd extensions/zed
cargo build --release --target wasm32-wasip1
```

The compiled extension is `target/wasm32-wasip1/release/casual_review_zed.wasm`.

## Install (dev mode)

In Zed: **zed → Extensions → Install Dev Extension** and pick
`extensions/zed/`. Zed compiles the extension itself; you do not need to
run `cargo build` first when using the dev-install path.

## Slash commands

| Command | Purpose |
|---|---|
| `/cr-help` | Print the `cr comment` command reference. |
| `/cr-list` | Render `cr comment list --include-ancestors`. |
| `/cr-add <file>:<lines> <body…>` | Compose `cr comment add ... -m '<body>'`. Argument forms: `path/to/file.rs:42` or `path/to/file.rs:42:44`. |
| `/cr-reply <id> <body…>` | Compose `cr comment reply <id> -m '<body>'`. |
| `/cr-resolve <id> [<message…>]` | Compose `cr comment resolve <id>` (with optional `-m`). |
| `/cr-sync` | Render `cr fetch origin` + `cr push origin`. |
| `/cr-status` | Read `.cr-comments/*.json` if `cr` is in file-fallback mode. |

Each command's output is markdown — copy the fenced shell block into a
terminal to run it. Bodies with spaces or special characters are
single-quoted with `'\''` escapes for POSIX shells.

## Roadmap

A future version may declare a `cr-mcp` MCP **context server** in
`extension.toml`. Zed launches context servers as separate processes
outside the WASM sandbox, so an MCP server *could* shell out to `cr` and
expose comments to Zed's Assistant as live context. That requires
shipping a `cr-mcp` binary, which is tracked separately from this
extension.
