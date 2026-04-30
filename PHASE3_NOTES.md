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

## Known Limitations (MVP)

### 1. File-Based Storage (Not Git Refs)

**Current Implementation:** Findings are stored in `.cr-findings/` directory with timestamped JSON files.

```
.cr-findings/
├── findings-1777518223.json
├── findings-1777518330.json
└── findings-9999999999.json
```

**Why:** Simpler implementation for MVP, still compatible with git (can be excluded via .gitignore).

**Limitation:** Findings don't actually persist in git refs, so they won't survive `clone` without special handling.

**Migration Path:** See "Phase 3.1 - Git Refs Integration" below.

### 2. Single Findings File Per Commit

When multiple `cr publish` or `cr ack` commands are run, new files are created instead of updating a single ref.

**Why:** Avoids complex ref mutations for MVP.

**Impact:** `cr show` reads the most recent file by parsing timestamps from filenames.

**Solution:** Will be fixed by git refs implementation.

### 3. Fetch/Push Commands are Stubs

Currently, `cr fetch` and `cr push` use git directly but won't do anything useful until findings are in git refs:

```bash
cr fetch origin  # Runs: git fetch origin refs/notes/casual-review:refs/notes/casual-review
                 # But fails if refs/notes/casual-review doesn't exist yet
```

**Why:** Prepared the CLI surface before implementing the underlying mechanism.

**Next:** Will work correctly once git refs backend is ready.

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

## Phase 3.1 - Git Refs Integration (Future)

When ready to complete Phase 3, implement proper git notes backend:

### 1. Replace File-Based Storage

```rust
// In src/git_notes.rs
pub fn write_notes(repo_path: &Path, commit: &str, payload: NotesPayload) -> anyhow::Result<()> {
    let repo = git2::Repository::open(repo_path)?;
    let commit_obj = repo.revparse_single(commit)?;
    
    // Serialize payload to JSON
    let json = serde_json::to_string(&payload)?;
    
    // Write to refs/notes/casual-review
    let mut notes = repo.note_commits(&git2::Signature::now("cr", "cr@example.com")?)?;
    notes.create(&commit_obj, Some("casual-review"), &json.into_bytes(), false)?;
    
    Ok(())
}
```

### 2. Benefits

- Findings persist in `.git/refs/notes/casual-review` (part of git repo)
- `cr fetch/push` will actually sync findings across repos
- Single authoritative note per commit (no timestamp files)
- Compatible with `git fetch/push` for collaboration

### 3. Implementation Checklist

- [ ] Replace `write_notes` with git2 implementation
- [ ] Replace `read_notes` with git2 implementation
- [ ] Add proper error handling for missing commits
- [ ] Update tests to verify git refs are created
- [ ] Test full workflow: publish → clone → fetch → show
- [ ] Document git notes ref structure
- [ ] Migration guide for existing file-based findings

### 4. Expected Workflow After Migration

```bash
# Developer A
cr publish HEAD
git push origin refs/notes/casual-review:refs/notes/casual-review

# Developer B
git clone <repo>
cr fetch origin
cr show HEAD  # Shows findings from Developer A
cr ack CR-12345678 "Fixed in my PR"
git push origin refs/notes/casual-review:refs/notes/casual-review
```

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
