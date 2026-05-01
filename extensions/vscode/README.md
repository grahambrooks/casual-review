# Casual Review — VS Code Extension

Thin client for [casual-review](../..). Reads, writes, and syncs review comments
stored in `refs/notes/casual-review/discuss` by shelling out to the `cr` CLI.

## Requirements

- `cr` on `PATH` (or set `casualReview.binPath` in settings).
- Phase 4.1+ of casual-review (i.e. `cr comment` subcommands present).
- A git repository open in the workspace root.

## Build (dev mode)

```sh
cd extensions/vscode
npm install
npm run compile
```

To run the extension under a development host of VS Code:

1. Open `extensions/vscode/` in VS Code.
2. Press `F5` (or run **Debug: Start Debugging**). A new "Extension Development
   Host" window opens with the extension loaded.
3. In that window, open a git repo and use the **Casual Review** commands from
   the command palette.

## Commands

| Command | What it does |
|---|---|
| `Casual Review: Add Comment on Selection` | Anchors a comment to the current selection (or current line) and `cr comment add`s it. Prompts for body. |
| `Casual Review: Reply to Comment` | Picks an existing comment from a list and `cr comment reply`s. |
| `Casual Review: Resolve Comment` | Picks an existing comment and `cr comment resolve`s. |
| `Casual Review: Sync (fetch + push)` | Runs `cr fetch` then `cr push`. |
| `Casual Review: Fetch` / `Casual Review: Push` | Each half of the sync. |
| `Casual Review: Refresh Comments` | Re-runs `cr comment list` and re-applies decorations. |
| `Casual Review: Show Stale Comments` | Lists all comments whose anchor bytes have drifted in the working tree. |

The extension automatically refreshes when the active editor changes or after
any mutating command. There is no daemon and no LSP — every action is one
`cr` invocation.

## Settings

| Key | Default | Notes |
|---|---|---|
| `casualReview.binPath` | `cr` | Override if `cr` isn't on the VS Code process's `PATH`. |
| `casualReview.includeAncestors` | `true` | Project comments from ancestor commits onto HEAD (passes `--include-ancestors`). |
| `casualReview.remote` | `origin` | Default remote for sync/fetch/push. |

## Limitations (v1)

- No real-time sync from teammates: run **Sync** manually.
- Stale detection is computed in-extension by hashing the anchored bytes; it
  matches `cr`'s logic but is not authoritative if `cr` ever changes the
  algorithm.
- Comments author identity comes from `git config user.{name,email}`. If those
  are unset, `cr comment add` fails — set them first.
- Anchoring is line-range based. Comments survive small edits within the same
  byte range; larger drifts mark them stale and require `cr comment reanchor`.

## Architecture

The extension is a thin shell. All persistence and sync go through `cr`. The
JSON contract is `casual-review/comment/1` from `cr comment list --format json`.
That contract is the only thing other editor extensions (JetBrains, Neovim)
need to honor.
