# Git as a substrate for developer-to-developer and agent-to-developer communication

A working document on the question: how do you share context, advice, opinions, or
review-style suggestions about code — the kind of thing that doesn't warrant a full
issue or defect — using the tools Git already provides? Triggered by an interest in
`git notes` and broadened to a survey of the surrounding mechanisms, including a
deep look at `git-appraise` as the most worked-out example of treating "stuff you
want to say about code" as a first-class Git artifact.

## Framing

Git was designed around content-addressable storage, and most of its sharing
mechanisms grew from that root. The features relevant for human-to-human or
human-to-agent commentary around code fall into a few distinct buckets — some
obvious, some commonly overlooked. The trade-offs ultimately come down to three
axes:

- **Discoverability.** Should the annotation be visible to anyone running
  `git log` (commit messages and trailers do this), or only to people who
  explicitly opt in to fetch it (notes, custom refs)?
- **Mutability.** Does it need to change over time without rewriting history?
  Notes win here; trailers lose.
- **Audience.** Does the audience include agents that need to parse it?
  That favours structured formats — YAML or JSON inside a notes ref, or a tracked
  file with a schema — over freeform prose.

## Git notes

Notes are the most direct mechanism for the use case. They attach arbitrary text
to any Git object — typically commits, but also blobs and trees — without
changing the object's hash. The data lives in its own ref namespace
(`refs/notes/commits` by default), which means notes can be added, edited, or
deleted retroactively without rewriting history.

Multiple "channels" are possible by using different refs:

```
git notes --ref=review add
git notes --ref=ai-context add
```

Notes don't propagate by default. They have to be pushed and fetched explicitly
with refspecs like `refs/notes/*:refs/notes/*`, which is the main reason most
teams never discover them.

**Caveat for line-level use cases:** notes attach to whole objects, not to
specific lines. There's no built-in way to say "this advice is about line 42."
You either attach to the blob (the file at that version) and reference the line
in the note body, or attach to the commit. Tools like `git-appraise` build a
line-level review system on top of notes by encoding line numbers inside the
note payload.

## Commit message trailers

Trailers are the structured-metadata mechanism most teams already use without
thinking about it. Lines like `Signed-off-by:`, `Reviewed-by:`,
`Co-authored-by:`, `Refs:`, `Closes:` parse cleanly with
`git interpret-trailers`.

This is where AI co-authorship is increasingly being recorded —
`Co-authored-by: Claude <noreply@anthropic.com>` is the convention GitHub picks
up for attribution — and it's a natural home for rationale, links to discussion,
or `Advice-from:` style annotations if a team adopts a convention.

Drawback: trailers belong to the commit, so they're frozen once the commit is
made. Changing them requires a rebase, which rewrites history.

## Annotated tags

Tags carry their own message and can be signed. They're underused for anything
beyond releases, but in principle nothing stops you from tagging a commit with
a tag whose message contains a discussion or piece of advice. Like notes, they
can live in custom namespaces and be pushed selectively.

## Custom refs

`refs/` is Git's general-purpose escape hatch — anything under it is just a
named pointer to an object. Gerrit (`refs/changes/*`), git-appraise (review
threads stored as commits), and Gitea's PR mirroring all exploit this.

You could design a `refs/agents/<sha>` scheme pointing to a tree of structured
advice files and push it alongside the code. More work than notes, but it gives
you arbitrary structure and history.

## Replace refs

`git replace` lets you transparently substitute one object for another at read
time. It's a sharp tool, mainly used for grafts and history surgery, but it's
another channel that exists.

## In-repo conventions

A lot of social information actually flows through tracked files rather than
Git plumbing:

- `.git-blame-ignore-revs` — skip formatting commits in `git blame`.
- `.mailmap` — canonicalize identities across name and email changes.
- `CODEOWNERS` — "talk to this person about this code."
- `.gitattributes` — per-path metadata.

These aren't Git features so much as files Git tooling agrees to consult. They
travel with the repo, get reviewed like code, and have the same lifecycle as
the code.

## Format-patch / am

The kernel-style email workflow preserves threading naturally: a patch carries
the commit message, optional discussion below `---` (which `git am` strips), and
travels through a medium that already supports replies. The
discussion-around-a-patch is itself an artifact.

It's archaic for most teams but worth knowing as a contrast — the discussion
never lived in Git, but it lived in something durable and linkable.

## Emerging patterns for agents

The ground here is shifting fast. A few patterns worth betting on:

**Repo-level context files** — `CLAUDE.md`, `AGENTS.md`, `.cursor/rules/` —
that travel with the code as ordinary tracked files. Simple, durable, easy for
any tool to find. This is winning because it requires no special infrastructure.

**`Co-authored-by` trailers for AI-assisted commits**, which gives a queryable
audit trail with `git log --grep`.

**Architecture Decision Records (ADRs)** as committed markdown. These solve the
"advice that doesn't warrant an issue" problem by giving rationale a permanent
home next to the code, with the same review and history as the code itself.

**Inline source comments with structured tags** (`// AI-NOTE:`, `// CONTEXT:`).
Easy, but they pollute the source and have no lifecycle.

For most teams a hybrid will probably win: tracked context files for the durable
stuff, trailers for attribution, and notes for the ephemeral "here's a thought"
layer that doesn't deserve to be carved into the commit.

---

## Deep dive: git-appraise

`git-appraise` is the most ambitious attempt to make code review itself live
inside Git, instead of in a forge's database. Originally built by Google
engineers (Omar Jarjur and others), open-sourced around 2016, written in Go,
and shipped as a `git appraise` subcommand. Source:
[github.com/google/git-appraise](https://github.com/google/git-appraise).

### The core idea

A code review on most platforms lives in the platform: GitHub stores PR
comments in its own database, GitLab stores MR threads in its own database, and
if the platform goes away or a project migrates, the review history goes with
it. git-appraise's pitch is that review data is just data; the code already
lives in a distributed version-control system that's good at syncing arbitrary
objects, so put the review in there too.

The consequence: you can clone a repo and get the entire review history
offline, push it to a different remote, mirror it between forges, or work on a
review without any server at all.

### Storage model

git-appraise leans heavily on Git notes, in three separate ref namespaces:

**`refs/notes/devtools/reviews`** holds the review request itself — a JSON blob
attached as a note to the head commit of a review branch. It records the target
branch, reviewers, status, and description. Creating a review with
`git appraise request` writes a note on the head commit; that note is the
review.

**`refs/notes/devtools/discuss`** holds comments. Each comment is its own JSON
object with fields for author, timestamp, body, optional location (file path,
line range, commit), and a parent hash. Threading is just "this comment's
parent points to the SHA of another comment" — threads are a DAG, the same
way Git history is. Replies are first-class.

**`refs/notes/devtools/analyses`** holds machine-generated reports — CI
status, static analysis output, anything an automated reviewer wants to attach.
Same shape: JSON blobs attached as notes.

Because notes are themselves Git objects, every comment and every review state
has a SHA, can be signed, and has a verifiable history. Mutability is handled
by appending: editing a comment means writing a new comment with the same
parent, and the tooling shows the latest in the chain.

### The CLI surface

The commands roughly mirror what you'd expect from a forge:

- `git appraise request` — open a review on the current branch.
- `git appraise list` — show open reviews.
- `git appraise show` — display a review with its comment thread.
- `git appraise comment` — add a comment, optionally scoped to a file, line,
  or commit.
- `git appraise accept` / `reject` — record approval.
- `git appraise submit` — land the change.
- `git appraise pull` / `push` — sync review data alongside the code via
  standard Git remotes.

The sync commands are what show off the model: there's no special server. Any
Git host that lets you push refs under `refs/notes/*` works, including a bare
repo on a USB stick.

### Why it matters even if nobody uses it

git-appraise demonstrates that line-level review is achievable on top of git
notes — which by themselves only attach to whole objects. The trick: the
location lives inside the note's JSON payload, not in Git's addressing. A
comment says "I'm about file `foo.go`, lines 42–47, of commit `abc123`," and
the tooling renders that. Git doesn't know or care; it's just storing
structured text.

This is the same trick that would be used to build agent-to-developer
annotations on top of notes today. Notes don't need to support line ranges —
they need a schema for the note body that records the location, and a renderer
that reads it.

### Honest assessment

git-appraise has been quiet for a long time. Activity on the upstream repo has
been sparse for years, the GitHub/GitLab integrations never really took hold,
and most teams' review workflow is tied to whichever forge they use. There's a
related ecosystem — `git-pull-request-mirror` to bridge GitHub PRs into
appraise format, various forks — but none of it has critical mass.

The reasons are mostly social rather than technical. Review tooling lives where
the conversation lives, and the conversation lives where the platform's
notifications go. A distributed review system that nobody gets emailed about is
a tree falling in an empty forest. Forges also keep adding review features
(suggestions, draft PRs, merge queues) that a generic distributed system has to
chase.

That said, the design is still worth studying for the agent-context problem.
It's the most worked-out example of treating "stuff you want to say about code,
separately from the code" as a first-class Git artifact. The schema choices —
JSON-in-notes, hash-parent threading, separate refs for separate kinds of
metadata — are reusable even if the tool itself isn't where you end up.

If sketching an "agent advice" system today, the practical starting point is
to read git-appraise's `review/` and `comment/` Go packages and ask which of
those decisions still apply when the author is an LLM rather than a human.

## Practical takeaways

- For durable, repo-wide agent context: tracked files (`CLAUDE.md`,
  `AGENTS.md`, ADRs in `docs/adr/`).
- For attribution and audit trail: `Co-authored-by:` trailers in commit
  messages.
- For mutable, opt-in advice attached to specific commits: a custom
  notes ref (e.g. `refs/notes/agent-advice`) with a small JSON schema in the
  note body.
- For line-level granularity on top of notes: encode the location in the
  schema, the way git-appraise does. Don't expect Git to address lines for
  you.
- Forge integration is the social bottleneck, not the technical one. Anything
  that lives only in custom refs has to fight for visibility.
