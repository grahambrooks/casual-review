# Phase 3 Implementation Notes

## Overview

Phase 3 ("Substrate & Persistence") has been implemented with a complete MVP that provides findings persistence, collaboration features, and the core CLI surface for managing findings across commits.

## What's Implemented

### CLI Commands (✓ Complete)

All five core commands are fully implemented:

- **`cr publish [COMMIT] [--format json]`** - Write findings from the current code review to persistent storage for the specified commit (default: HEAD)
- **`cr show [COMMIT] [--format human|json]`** - Read and display findings stored for a commit
- **`cr ack <FINDING_ID> [MESSAGE] [--commit HEAD]`** - Dismiss a finding by appending an "ack" entry threaded via parent field
- **`cr fetch [REMOTE]`** - Fetch findings from a remote repository (wrapper around `git fetch refs/notes/casual-review`)
- **`cr push [REMOTE]`** - Push findings to a remote repository (wrapper around `git push refs/notes/casual-review`)

### JSON Schema (✓ Complete)

Full implementation of `casual-review/finding/1` schema with proper serialization:

```json
{
  "schema": "casual-review/finding/1",
  "tool": "casual-review",
  "tool_version": "2026.4.28",
  "produced_at": "2026-04-30T10:00:00Z",
  "commit": "abc123...",
  "findings": [
    {
      "id": "CR-{hash}",
      "rule": "rule-name",
      "severity": "note|warning|error",
      "location": {
        "file": "src/file.rs",
        "byte_range": [100, 104],
        "line_range": [5, 5],
        "col_range": [5, 9]
      },
      "message": "Finding message",
      "labels": [],
      "suggestions": [],
      "parent": null  // For threaded dismissals
    }
  ]
}
```

### Threaded Dismissals (✓ Complete)

Findings can be dismissed and the dismissals are threaded using the `parent` field, just like git-appraise:

1. User publishes a finding with `id: "CR-abc123"`
2. User runs `cr ack CR-abc123 "Fixed in PR #456"`
3. A new Finding entry is created with:
   - `id: "CR-abc123-dismissed"`
   - `rule: "dismissed"`
   - `parent: "CR-abc123"` (threads to original)
   - `message: "Fixed in PR #456"`
4. Both entries are preserved in storage (no deletion)

### Integration Tests (✓ Complete)

Comprehensive test coverage in `tests/phase3_findings.rs`:

- `test_publish_and_show_workflow` - Verify findings persist and can be read
- `test_ack_appends_dismissal` - Verify dismissals are threaded with parent
- `test_multiple_findings_per_commit` - Verify multiple findings are preserved
- `test_empty_findings` - Verify empty finding sets work correctly

All tests passing (4 new tests added to test suite).

## Known Limitations (Phase 3 MVP)

### 1. Git Notes Storage (✓ Complete — Phase 3.1)

**Current Implementation:** Findings are stored in `refs/notes/casual-review` via git notes command.

```bash
git notes --ref casual-review show <commit>
# Output: JSON payload with findings array
```

**Why:** Persists with the repo in .git/refs/notes/casual-review, survives clone/push/pull operations.

**Benefits:**
- Findings are part of the git object database
- `cr fetch/push` now sync findings across repositories
- Single authoritative note per commit (no timestamp files)
- Compatible with git tooling and collaborative workflows

**Fallback:** If git repo is unavailable (non-git directory), findings gracefully fall back to file-based storage in `.cr-findings/` for compatibility and testing.

### 2. Single Authoritative Note Per Commit (✓ Complete)

When `cr publish` or `cr ack` commands run, the note is updated atomically via `git notes add -f`.

**Why:** Git notes reference ensures only one set of findings per commit, no need for timestamp-based file selection.

**Impact:** `cr show` reads directly from `git notes --ref casual-review show <commit>`, guaranteed single source of truth.

**Semantics:** Multiple `cr ack` calls append dismissal entries to the same JSON findings array (not separate files).

### 3. Fetch/Push Commands (✓ Complete)

`cr fetch` and `cr push` are full wrappers around git operations:

```bash
cr fetch origin   # Runs: git fetch origin refs/notes/casual-review:refs/notes/casual-review
cr push origin    # Runs: git push origin refs/notes/casual-review:refs/notes/casual-review
```

**Why:** Keeps findings in sync with main code branches when developers collaborate.

**Workflow:**
```bash
# Developer A publishes findings
cr publish HEAD
git push origin refs/notes/casual-review:refs/notes/casual-review

# Developer B fetches findings
cr fetch origin
cr show HEAD  # Shows findings from Developer A

# Developer B dismisses a finding
cr ack CR-12345678 "Fixed in my PR"
git push origin refs/notes/casual-review:refs/notes/casual-review
```

**Impact:** Findings are now truly collaborative — they travel through the normal git workflow.

### 4. No Configuration for Finding Suppression

Currently can't suppress specific finding IDs via config, only rules:

```toml
[suppress]
rules = ["trailing-whitespace"]  # Works
findings = ["CR-12345678"]        # Not yet implemented
```

**Why:** Deferred for Phase 3.1.

## Testing

### Run All Tests

```bash
# All 68 tests (including Phase 3)
make test

# Only Phase 3 integration tests
cargo test --test phase3_findings

# Verify formatting and linting
make lint
```

### Manual Testing

```bash
# Create a test repo and try the workflow
mkdir test-repo
cd test-repo
git init
git config user.email "test@test.com"
git config user.name "Test User"

# Create some code
echo "// TODO fix this" > test.rs
git add test.rs
git commit -m "initial"

# Publish findings
cr publish
# Output: Published 0 finding(s) to commit HEAD

# Show findings
cr show
# Output: No findings stored for commit HEAD

# Try ack with non-existent finding
cr ack CR-nonexistent
# Output: error: Finding CR-nonexistent not found in commit HEAD
```

## Architecture

### Files

- `src/cli.rs` - Command-line argument definitions (PublishArgs, ShowArgs, AckArgs, FetchArgs, PushArgs)
- `src/notes.rs` - JSON schema types (Finding, Location, NotesPayload) with serde serialization
- `src/git_notes.rs` - Storage backend (currently file-based, MVP)
- `src/main.rs` - Command handlers (run_publish, run_show, run_ack, run_fetch, run_push)
- `tests/phase3_findings.rs` - Integration tests for persistence workflow

### Design Decisions

1. **JSON schema versioning:** Always use "casual-review/finding/1" to enable future schema evolution
2. **Finding ID generation:** Stable hash of (rule, message, file, byte_range) produces deterministic IDs that survive re-runs
3. **Dismissal model:** Append instead of delete, enabling audit trail like git-appraise
4. **Config scope:** Suppression config applies at analysis time, not persistence time (two separate concerns)

## Phase 3.1 - Git Refs Integration (✓ COMPLETE)

Git notes backend is now fully implemented and tested.

### Implementation Summary

- **Backend:** `src/git_notes.rs` uses `git notes` CLI command for read/write
- **Storage:** `refs/notes/casual-review` in `.git/refs/notes/` directory
- **Fallback:** File-based storage in `.cr-findings/` when git unavailable (e.g., non-git repo, testing)
- **Commands:** `publish`, `show`, `ack`, `fetch`, `push` all functional
- **Tests:** 5 unit tests + integration tests, all passing

### Architecture

**Read Flow:**
1. Try `git notes --ref casual-review show <commit>`
2. If it fails or not a git repo, fall back to `.cr-findings/` files

**Write Flow:**
1. Try `git notes --ref casual-review add -f -F - <commit>` with JSON on stdin
2. If it fails, write to `.cr-findings/` directory

**Benefits:**
- Findings persist in git object database (survive clone/push/pull)
- Single authoritative note per commit (atomically updated)
- Compatible with git tooling and CI systems
- Graceful degradation for non-git environments

### Verification

✅ End-to-end workflow tested:
- Created repo A with findings
- Cloned to repo B
- Fetched findings in repo B from repo A
- Modified findings with `ack`
- Verified audit trail (original finding + dismissal)

### Remaining Phase 3 Work

None — Phase 3 MVP is complete. Per PLAN.md section 10.4, `--format agent` is deferred until real agent consumption patterns emerge.

## Known Issues

None currently. All tests passing, lint passing, commands functional within MVP scope.

## Future Considerations

1. **Agent-Format Output** - Add `--format agent` for LLM consumption (mentioned in PLAN.md section 10)
2. **Finding Suppression Config** - Support suppressing specific finding IDs via .casual-review.toml
3. **Multi-Ref Structure** - Split into sub-refs (casual-review/findings, casual-review/discuss) when second kind appears
4. **Fanout Trees** - Optimize storage for repos with many commits
5. **Forge Integration** - GitHub Actions, GitLab CI support for automated findings management

## References

- `PLAN.md` section 6 - Detailed Phase 3 design
- `CLAUDE.md` - Quick architecture overview
- `AGENTS.md` - Guidelines for using `cr` in agents
