import * as vscode from "vscode";
import { execFile } from "child_process";
import { promisify } from "util";
import * as path from "path";
import * as crypto from "crypto";

const execFileAsync = promisify(execFile);

interface Author {
  name: string;
  email: string;
}

interface Anchor {
  file?: string;
  line_range: [number, number];
  byte_range: [number, number];
  anchor_text_sha: string;
}

interface Comment {
  id: string;
  author: Author;
  created_at: string;
  anchor: Anchor;
  body: string;
  parent?: string;
  resolved?: boolean;
  origin_commit?: string;
}

interface CommentsPayload {
  schema: string;
  tool: string;
  tool_version: string;
  commit: string;
  comments: Comment[];
}

let cachedPayload: CommentsPayload | undefined;
let staleIds: Set<string> = new Set();
let statusBar: vscode.StatusBarItem;
let outputChannel: vscode.OutputChannel;
let commentsTree: CommentsTreeProvider;

const freshDecoration = vscode.window.createTextEditorDecorationType({
  isWholeLine: true,
  borderWidth: "0 0 0 3px",
  borderStyle: "solid",
  borderColor: new vscode.ThemeColor("editorWarning.foreground"),
  overviewRulerColor: new vscode.ThemeColor("editorWarning.foreground"),
  overviewRulerLane: vscode.OverviewRulerLane.Right,
});

const staleDecoration = vscode.window.createTextEditorDecorationType({
  isWholeLine: true,
  borderWidth: "0 0 0 3px",
  borderStyle: "dotted",
  borderColor: new vscode.ThemeColor("editorError.foreground"),
  overviewRulerColor: new vscode.ThemeColor("editorError.foreground"),
  overviewRulerLane: vscode.OverviewRulerLane.Right,
});

export function activate(context: vscode.ExtensionContext) {
  outputChannel = vscode.window.createOutputChannel("Casual Review");
  context.subscriptions.push(outputChannel);

  statusBar = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Right,
    100,
  );
  statusBar.command = "casualReview.refresh";
  context.subscriptions.push(statusBar);

  commentsTree = new CommentsTreeProvider();
  context.subscriptions.push(
    vscode.window.registerTreeDataProvider("casualReview.comments", commentsTree),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("casualReview.refresh", refresh),
    vscode.commands.registerCommand("casualReview.refreshTree", refresh),
    vscode.commands.registerCommand("casualReview.addComment", addComment),
    vscode.commands.registerCommand("casualReview.replyToComment", replyToComment),
    vscode.commands.registerCommand("casualReview.resolveComment", resolveComment),
    vscode.commands.registerCommand("casualReview.openComment", openComment),
    vscode.commands.registerCommand("casualReview.fetch", fetchRemote),
    vscode.commands.registerCommand("casualReview.push", pushRemote),
    vscode.commands.registerCommand("casualReview.sync", sync),
    vscode.commands.registerCommand("casualReview.syncTree", sync),
    vscode.commands.registerCommand("casualReview.showStale", showStale),
  );

  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor((editor) => {
      if (editor) applyDecorations(editor);
    }),
    vscode.workspace.onDidSaveTextDocument(() => refresh()),
  );

  // Initial load.
  refresh().catch((e) => log(`initial refresh failed: ${e}`));
}

export function deactivate() {
  freshDecoration.dispose();
  staleDecoration.dispose();
}

function log(msg: string) {
  outputChannel.appendLine(`[${new Date().toISOString()}] ${msg}`);
}

function getConfig() {
  const cfg = vscode.workspace.getConfiguration("casualReview");
  return {
    binPath: cfg.get<string>("binPath") || "cr",
    includeAncestors: cfg.get<boolean>("includeAncestors") ?? true,
    remote: cfg.get<string>("remote") || "origin",
  };
}

function workspaceRoot(): string | undefined {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) return undefined;
  return folders[0].uri.fsPath;
}

async function runCr(args: string[]): Promise<{ stdout: string; stderr: string }> {
  const cwd = workspaceRoot();
  if (!cwd) throw new Error("No workspace folder open");
  const { binPath } = getConfig();
  log(`$ ${binPath} ${args.join(" ")}`);
  try {
    const { stdout, stderr } = await execFileAsync(binPath, args, { cwd });
    if (stderr.trim()) log(`stderr: ${stderr.trim()}`);
    return { stdout, stderr };
  } catch (e: unknown) {
    const err = e as { stdout?: string; stderr?: string; message?: string };
    const msg = err.stderr?.trim() || err.message || String(e);
    log(`error: ${msg}`);
    throw new Error(msg);
  }
}

async function refresh(): Promise<void> {
  const cwd = workspaceRoot();
  if (!cwd) return;
  const { includeAncestors } = getConfig();
  const args = ["comment", "list", "--format", "json"];
  if (includeAncestors) args.push("--include-ancestors");

  try {
    const { stdout } = await runCr(args);
    if (!stdout.trim()) {
      cachedPayload = undefined;
    } else {
      cachedPayload = JSON.parse(stdout) as CommentsPayload;
    }
  } catch {
    cachedPayload = undefined;
  }

  await recomputeStaleness(cwd);

  const editor = vscode.window.activeTextEditor;
  if (editor) applyDecorations(editor);
  updateStatusBar();
  commentsTree?.refresh();
}

async function recomputeStaleness(cwd: string) {
  staleIds.clear();
  if (!cachedPayload) return;
  for (const c of cachedPayload.comments) {
    if (!c.anchor.file || !c.anchor.anchor_text_sha) continue;
    const abs = path.join(cwd, c.anchor.file);
    let bytes: Buffer;
    try {
      bytes = await vscode.workspace.fs.readFile(vscode.Uri.file(abs)).then(
        (u) => Buffer.from(u),
      );
    } catch {
      staleIds.add(c.id);
      continue;
    }
    const slice =
      c.anchor.line_range[0] === 0 && c.anchor.line_range[1] === 0
        ? bytes
        : bytes.subarray(c.anchor.byte_range[0], c.anchor.byte_range[1]);
    if (slice.length < c.anchor.byte_range[1] - c.anchor.byte_range[0]) {
      staleIds.add(c.id);
      continue;
    }
    const sha = crypto.createHash("sha256").update(slice).digest("hex");
    if (sha !== c.anchor.anchor_text_sha) staleIds.add(c.id);
  }
}

function applyDecorations(editor: vscode.TextEditor) {
  if (!cachedPayload) {
    editor.setDecorations(freshDecoration, []);
    editor.setDecorations(staleDecoration, []);
    return;
  }
  const cwd = workspaceRoot();
  if (!cwd) return;
  const editorPath = path.normalize(
    path.relative(cwd, editor.document.uri.fsPath),
  );
  const fresh: vscode.DecorationOptions[] = [];
  const stale: vscode.DecorationOptions[] = [];

  // Visible threads only — hide anything whose root has a resolution record.
  const resolvedRoots = new Set<string>();
  for (const c of cachedPayload.comments) {
    if (c.resolved && c.parent) resolvedRoots.add(c.parent);
  }

  // Group replies under their root for the hover.
  const repliesByParent = new Map<string, Comment[]>();
  for (const c of cachedPayload.comments) {
    if (c.parent) {
      const list = repliesByParent.get(c.parent) ?? [];
      list.push(c);
      repliesByParent.set(c.parent, list);
    }
  }

  for (const c of cachedPayload.comments) {
    if (c.parent) continue; // only render roots
    const root = c.id;
    if (resolvedRoots.has(root)) continue;
    if (!c.anchor.file) continue;
    if (path.normalize(c.anchor.file) !== editorPath) continue;
    if (c.anchor.line_range[0] === 0 && c.anchor.line_range[1] === 0) continue;

    const startLine = Math.max(0, c.anchor.line_range[0] - 1);
    const endLine = Math.max(startLine, c.anchor.line_range[1] - 1);
    const range = new vscode.Range(startLine, 0, endLine, 0);
    const hover = renderThreadHover(c, repliesByParent.get(c.id) ?? []);
    const opts: vscode.DecorationOptions = { range, hoverMessage: hover };
    if (staleIds.has(c.id)) stale.push(opts);
    else fresh.push(opts);
  }

  editor.setDecorations(freshDecoration, fresh);
  editor.setDecorations(staleDecoration, stale);
}

function renderThreadHover(
  root: Comment,
  replies: Comment[],
): vscode.MarkdownString {
  const md = new vscode.MarkdownString();
  md.isTrusted = true;
  md.supportHtml = false;
  const flag =
    (staleIds.has(root.id) ? " · _stale_" : "") +
    (root.origin_commit ? ` · _from ${root.origin_commit.slice(0, 8)}_` : "");
  md.appendMarkdown(
    `**${escapeMd(root.author.name)}** · \`${root.id}\`${flag}\n\n`,
  );
  md.appendMarkdown(`${escapeMd(root.body)}\n\n`);
  for (const r of replies) {
    if (r.resolved) continue;
    md.appendMarkdown(
      `> **${escapeMd(r.author.name)}** · \`${r.id}\`\n> ${escapeMd(r.body).replace(/\n/g, "\n> ")}\n\n`,
    );
  }
  md.appendMarkdown(
    `[Reply](command:casualReview.replyToComment?${encodeURIComponent(JSON.stringify([root.id]))}) · ` +
    `[Resolve](command:casualReview.resolveComment?${encodeURIComponent(JSON.stringify([root.id]))})`,
  );
  return md;
}

function escapeMd(s: string): string {
  return s.replace(/([\\`*_{}\[\]()#+\-.!])/g, "\\$1");
}

function updateStatusBar() {
  if (!cachedPayload) {
    statusBar.text = "$(comment-discussion) cr: —";
    statusBar.tooltip = "casual-review: not loaded";
    statusBar.show();
    return;
  }
  const resolvedRoots = new Set<string>();
  for (const c of cachedPayload.comments) {
    if (c.resolved && c.parent) resolvedRoots.add(c.parent);
  }
  const open = cachedPayload.comments.filter(
    (c) => !c.parent && !resolvedRoots.has(c.id),
  ).length;
  const stale = [...staleIds].length;
  statusBar.text = `$(comment-discussion) cr: ${open}${stale ? ` ($(warning)${stale})` : ""}`;
  statusBar.tooltip = `casual-review: ${open} open thread(s)${stale ? `, ${stale} stale` : ""}`;
  statusBar.show();
}

async function addComment() {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    vscode.window.showWarningMessage("Open a file to comment on first.");
    return;
  }
  const cwd = workspaceRoot();
  if (!cwd) return;
  const filePath = path.relative(cwd, editor.document.uri.fsPath);

  const sel = editor.selection;
  const startLine = sel.start.line + 1;
  const endLine = sel.end.line + 1;
  const lines = startLine === endLine ? `${startLine}` : `${startLine}:${endLine}`;

  const body = await vscode.window.showInputBox({
    prompt: `Comment on ${filePath}:${lines}`,
    placeHolder: "What about this code?",
    ignoreFocusOut: true,
  });
  if (!body || !body.trim()) return;

  try {
    await runCr(["comment", "add", filePath, "--lines", lines, "-m", body]);
    await refresh();
    vscode.window.showInformationMessage(`Comment added on ${filePath}:${lines}`);
  } catch (e: unknown) {
    vscode.window.showErrorMessage(`cr comment add failed: ${(e as Error).message}`);
  }
}

async function pickComment(
  predicate: (c: Comment) => boolean,
  prompt: string,
): Promise<string | undefined> {
  if (!cachedPayload || cachedPayload.comments.length === 0) {
    vscode.window.showWarningMessage("No comments to pick from. Try Refresh.");
    return undefined;
  }
  const candidates = cachedPayload.comments.filter(predicate);
  if (candidates.length === 0) {
    vscode.window.showWarningMessage("No matching comments.");
    return undefined;
  }
  const items = candidates.map((c) => ({
    label: `${c.id} — ${c.author.name}`,
    description: c.anchor.file
      ? `${c.anchor.file}:${c.anchor.line_range[0]}`
      : "<commit>",
    detail: c.body.split("\n")[0].slice(0, 100),
    id: c.id,
  }));
  const picked = await vscode.window.showQuickPick(items, { placeHolder: prompt });
  return picked?.id;
}

/**
 * `--commit <sha>` args for a comment id, when the comment was projected from
 * an ancestor commit. Its note lives on `origin_commit`, not HEAD, so reply /
 * resolve must target that commit or `cr` reports "No comments on commit HEAD".
 */
function commitArgsFor(id: string): string[] {
  const c = cachedPayload?.comments.find((c) => c.id === id);
  return c?.origin_commit ? ["--commit", c.origin_commit] : [];
}

async function replyToComment(passed?: string | TreeNode) {
  const id =
    nodeOrId(passed) ??
    (await pickComment((c) => !c.parent, "Reply to which comment?"));
  if (!id) return;
  const body = await vscode.window.showInputBox({
    prompt: `Reply to ${id}`,
    ignoreFocusOut: true,
  });
  if (!body || !body.trim()) return;
  try {
    await runCr(["comment", "reply", id, ...commitArgsFor(id), "-m", body]);
    await refresh();
  } catch (e: unknown) {
    vscode.window.showErrorMessage(`cr comment reply failed: ${(e as Error).message}`);
  }
}

async function resolveComment(passed?: string | TreeNode) {
  const id =
    nodeOrId(passed) ??
    (await pickComment((c) => !c.parent, "Resolve which comment?"));
  if (!id) return;
  const message = await vscode.window.showInputBox({
    prompt: `Resolution message for ${id} (optional)`,
    ignoreFocusOut: true,
  });
  try {
    const args = ["comment", "resolve", id, ...commitArgsFor(id)];
    if (message && message.trim()) args.push("-m", message);
    await runCr(args);
    await refresh();
  } catch (e: unknown) {
    vscode.window.showErrorMessage(`cr comment resolve failed: ${(e as Error).message}`);
  }
}

async function fetchRemote() {
  const { remote } = getConfig();
  try {
    await runCr(["fetch", remote]);
    await refresh();
    vscode.window.showInformationMessage(`Fetched from ${remote}`);
  } catch (e: unknown) {
    vscode.window.showErrorMessage(`cr fetch failed: ${(e as Error).message}`);
  }
}

async function pushRemote() {
  const { remote } = getConfig();
  try {
    await runCr(["push", remote]);
    vscode.window.showInformationMessage(`Pushed to ${remote}`);
  } catch (e: unknown) {
    vscode.window.showErrorMessage(`cr push failed: ${(e as Error).message}`);
  }
}

async function sync() {
  await fetchRemote();
  await pushRemote();
}

async function showStale() {
  if (!cachedPayload) await refresh();
  if (!cachedPayload || staleIds.size === 0) {
    vscode.window.showInformationMessage("No stale comments.");
    return;
  }
  const items = cachedPayload.comments
    .filter((c) => staleIds.has(c.id))
    .map((c) => ({
      label: `${c.id} — ${c.author.name}`,
      description: c.anchor.file
        ? `${c.anchor.file}:${c.anchor.line_range[0]}`
        : "<commit>",
      detail: c.body.split("\n")[0].slice(0, 100),
      id: c.id,
      anchor: c.anchor,
    }));
  const picked = await vscode.window.showQuickPick(items, {
    placeHolder: "Stale comments — pick to open the anchored line",
  });
  if (!picked || !picked.anchor.file) return;
  const cwd = workspaceRoot();
  if (!cwd) return;
  const uri = vscode.Uri.file(path.join(cwd, picked.anchor.file));
  const doc = await vscode.workspace.openTextDocument(uri);
  const editor = await vscode.window.showTextDocument(doc);
  const line = Math.max(0, picked.anchor.line_range[0] - 1);
  editor.revealRange(new vscode.Range(line, 0, line, 0), vscode.TextEditorRevealType.InCenter);
}

// --- Shared thread helpers ------------------------------------------------

/** Root ids whose thread has a resolution record and should be hidden. */
function resolvedRootIds(payload: CommentsPayload): Set<string> {
  const resolved = new Set<string>();
  for (const c of payload.comments) {
    if (c.resolved && c.parent) resolved.add(c.parent);
  }
  return resolved;
}

/** Replies grouped under their root id, oldest first. */
function repliesByParent(payload: CommentsPayload): Map<string, Comment[]> {
  const map = new Map<string, Comment[]>();
  for (const c of payload.comments) {
    if (!c.parent) continue;
    const list = map.get(c.parent) ?? [];
    list.push(c);
    map.set(c.parent, list);
  }
  for (const list of map.values()) {
    list.sort((a, b) => a.created_at.localeCompare(b.created_at));
  }
  return map;
}

/** Open, file-anchored root comments — the visible threads of the project. */
function visibleRoots(payload: CommentsPayload): Comment[] {
  const resolved = resolvedRootIds(payload);
  return payload.comments.filter(
    (c) => !c.parent && !resolved.has(c.id) && !!c.anchor.file,
  );
}

/** Normalize a command argument that may be a raw id or a tree node. */
function nodeOrId(passed?: string | TreeNode): string | undefined {
  if (!passed) return undefined;
  if (typeof passed === "string") return passed;
  if (passed.kind === "thread") return passed.comment.id;
  return undefined;
}

async function openComment(comment?: Comment) {
  if (!comment || !comment.anchor.file) return;
  const cwd = workspaceRoot();
  if (!cwd) return;
  const uri = vscode.Uri.file(path.join(cwd, comment.anchor.file));
  const doc = await vscode.workspace.openTextDocument(uri);
  const editor = await vscode.window.showTextDocument(doc);
  const line = Math.max(0, comment.anchor.line_range[0] - 1);
  const range = new vscode.Range(line, 0, line, 0);
  editor.selection = new vscode.Selection(range.start, range.start);
  editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
}

// --- Project-wide comments tree -------------------------------------------

type TreeNode =
  | { kind: "file"; file: string; threads: Comment[] }
  | { kind: "thread"; comment: Comment; replies: Comment[] };

class CommentsTreeProvider implements vscode.TreeDataProvider<TreeNode> {
  private readonly _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this._onDidChange.event;

  refresh(): void {
    this._onDidChange.fire();
  }

  getTreeItem(node: TreeNode): vscode.TreeItem {
    if (node.kind === "file") {
      const item = new vscode.TreeItem(
        node.file,
        vscode.TreeItemCollapsibleState.Expanded,
      );
      item.resourceUri = fileResourceUri(node.file);
      item.iconPath = vscode.ThemeIcon.File;
      const n = node.threads.length;
      item.description = `${n} thread${n === 1 ? "" : "s"}`;
      item.contextValue = "casualReview.file";
      return item;
    }

    const c = node.comment;
    const stale = staleIds.has(c.id);
    const label = c.body.split("\n")[0].slice(0, 80) || "(empty)";
    const item = new vscode.TreeItem(
      label,
      node.replies.length > 0
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None,
    );
    const line = c.anchor.line_range[0];
    item.description =
      `${c.author.name}${line ? ` · :${line}` : ""}` + (stale ? " · stale" : "");
    item.iconPath = new vscode.ThemeIcon(
      stale ? "warning" : "comment",
      stale ? new vscode.ThemeColor("editorError.foreground") : undefined,
    );
    item.tooltip = threadTooltip(c, node.replies);
    item.contextValue = "casualReview.thread";
    item.command = {
      command: "casualReview.openComment",
      title: "Open Comment",
      arguments: [c],
    };
    return item;
  }

  getChildren(node?: TreeNode): TreeNode[] {
    if (!cachedPayload) return [];

    if (!node) {
      // Top level: one node per file with open threads, sorted by path.
      const roots = visibleRoots(cachedPayload);
      const byFile = new Map<string, Comment[]>();
      for (const c of roots) {
        const file = path.normalize(c.anchor.file!);
        const list = byFile.get(file) ?? [];
        list.push(c);
        byFile.set(file, list);
      }
      return [...byFile.entries()]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([file, threads]) => ({
          kind: "file" as const,
          file,
          threads: threads.sort(
            (a, b) => a.anchor.line_range[0] - b.anchor.line_range[0],
          ),
        }));
    }

    if (node.kind === "file") {
      const replies = repliesByParent(cachedPayload);
      return node.threads.map((comment) => ({
        kind: "thread" as const,
        comment,
        replies: (replies.get(comment.id) ?? []).filter((r) => !r.resolved),
      }));
    }

    // Thread node: show its replies as leaves.
    return node.replies.map((reply) => ({
      kind: "thread" as const,
      comment: reply,
      replies: [],
    }));
  }
}

function fileResourceUri(file: string): vscode.Uri | undefined {
  const cwd = workspaceRoot();
  if (!cwd) return undefined;
  return vscode.Uri.file(path.join(cwd, file));
}

function threadTooltip(root: Comment, replies: Comment[]): vscode.MarkdownString {
  const md = new vscode.MarkdownString();
  md.isTrusted = true;
  const flag =
    (staleIds.has(root.id) ? " · _stale_" : "") +
    (root.origin_commit ? ` · _from ${root.origin_commit.slice(0, 8)}_` : "");
  md.appendMarkdown(`**${escapeMd(root.author.name)}** · \`${root.id}\`${flag}\n\n`);
  md.appendMarkdown(`${escapeMd(root.body)}\n\n`);
  for (const r of replies) {
    if (r.resolved) continue;
    md.appendMarkdown(
      `> **${escapeMd(r.author.name)}** · \`${r.id}\`\n> ${escapeMd(r.body).replace(/\n/g, "\n> ")}\n\n`,
    );
  }
  return md;
}
