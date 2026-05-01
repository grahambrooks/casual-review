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

  context.subscriptions.push(
    vscode.commands.registerCommand("casualReview.refresh", refresh),
    vscode.commands.registerCommand("casualReview.addComment", addComment),
    vscode.commands.registerCommand("casualReview.replyToComment", replyToComment),
    vscode.commands.registerCommand("casualReview.resolveComment", resolveComment),
    vscode.commands.registerCommand("casualReview.fetch", fetchRemote),
    vscode.commands.registerCommand("casualReview.push", pushRemote),
    vscode.commands.registerCommand("casualReview.sync", sync),
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

async function replyToComment(passedId?: string) {
  const id =
    passedId ?? (await pickComment((c) => !c.parent, "Reply to which comment?"));
  if (!id) return;
  const body = await vscode.window.showInputBox({
    prompt: `Reply to ${id}`,
    ignoreFocusOut: true,
  });
  if (!body || !body.trim()) return;
  try {
    await runCr(["comment", "reply", id, "-m", body]);
    await refresh();
  } catch (e: unknown) {
    vscode.window.showErrorMessage(`cr comment reply failed: ${(e as Error).message}`);
  }
}

async function resolveComment(passedId?: string) {
  const id =
    passedId ?? (await pickComment((c) => !c.parent, "Resolve which comment?"));
  if (!id) return;
  const message = await vscode.window.showInputBox({
    prompt: `Resolution message for ${id} (optional)`,
    ignoreFocusOut: true,
  });
  try {
    const args = ["comment", "resolve", id];
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
