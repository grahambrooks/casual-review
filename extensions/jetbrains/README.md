# Casual Review — JetBrains Plugin

Thin JetBrains client for [casual-review](../..). Reads, writes, and syncs
review comments stored in `refs/notes/casual-review/discuss` by shelling out
to the `cr` CLI. Compatible with any JetBrains IDE that bundles the platform
modules (IntelliJ IDEA, PyCharm, WebStorm, RustRover, etc.).

## Requirements

- JDK 21+ (build verified with the bundled wrapper on JDK 21–25).
  The IntelliJ Platform 2024.2 sandbox runs on its own bundled JBR, so the
  host JDK only matters for the build daemon.
- `cr` on PATH or set in **Settings → Tools → Casual Review**.
- A git repo open as the project root.

## Toolchain (pinned)

- Gradle 9.0 (via wrapper)
- Kotlin 2.2.0
- IntelliJ Platform Gradle Plugin 2.6.0
- Target IDE: IntelliJ IDEA Community 2024.2 (`sinceBuild = 242`,
  `untilBuild = 243.*`)

## Build

```sh
cd extensions/jetbrains
./gradlew buildPlugin
```

The first run downloads the IntelliJ Platform artifacts (~hundreds of MB)
into `~/.gradle/caches`. Output `.zip` lands in `build/distributions/`.

## Run in a sandbox IDE

```sh
./gradlew runIde
```

Launches a fresh IntelliJ Community sandbox with the plugin pre-installed.

## Install the built plugin

In any compatible JetBrains IDE: **Settings → Plugins → ⚙ → Install Plugin
from Disk…** and pick `build/distributions/casual-review-jetbrains-*.zip`.

## Actions

All actions live under **Tools → Casual Review** and in the editor context
menu (for `Add Comment`).

| Action | Behavior |
|---|---|
| Add Comment on Selection | Anchors a new comment to the current selection (or current line) and runs `cr comment add`. |
| Reply to Comment… | Picks an open thread and runs `cr comment reply`. |
| Resolve Comment… | Picks an open thread and runs `cr comment resolve`. |
| Refresh Comments | Re-runs `cr comment list` and redraws decorations. |
| Sync (Fetch + Push) | `cr fetch` then `cr push` against the configured remote. |
| Fetch / Push | Each half of the sync. |
| Show Stale Comments | Lists comments whose anchor bytes have drifted; pick to jump to the line. |

The status bar shows `cr: <open> [⚠<stale>]`. Click it to refresh.

## Settings

**Settings → Tools → Casual Review**

| Field | Default | Notes |
|---|---|---|
| `cr binary path` | `cr` | Override if `cr` isn't on the IDE's PATH. |
| `Default remote` | `origin` | Used by Sync / Fetch / Push. |
| `Include ancestor commits` | on | Passes `--include-ancestors` to `cr comment list`. |

## Architecture

Mirrors the VS Code extension. The plugin is a thin shell:

- `CrCli` shells out to `cr` and parses JSON via Gson (bundled).
- `CrService` (project-scoped) caches the latest payload, recomputes
  staleness in-process via SHA-256 over the anchored byte range, and
  applies `MarkupModel.addLineHighlighter` decorations on every open editor.
- Actions are plain `AnAction` subclasses that prompt for input via
  `Messages` and shell out via `CrCli`.
- The status-bar widget reads from `CrService` and updates on
  `WindowManager#getStatusBar(project).updateWidget(...)`.

The JSON contract (`casual-review/comment/1` from `cr comment list --format
json`) is the only thing this plugin pins to. If the schema evolves, only
`Model.kt` changes.

## Limitations (v1)

- No real-time refresh from teammates pushing — manual **Sync** required.
- Stale detection happens in the IDE; if `cr` ever changes its hashing
  algorithm, the two sides could diverge. (We use SHA-256 over the byte
  range; trivial.)
- Comments outside the project tree are ignored.
- File-level (`line_range = [0, 0]` with a file) and commit-level
  (`anchor.file = null`) comments are tracked by the service but not
  rendered in the gutter — they show in **Show Stale** and
  **Reply / Resolve** pickers.

## Plugin marketplace

`buildPlugin` produces a sideload-ready zip. JetBrains Marketplace publish is
a separate setup (vendor account, signing, etc.) and is not wired here.
