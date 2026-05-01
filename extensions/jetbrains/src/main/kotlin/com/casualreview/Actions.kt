package com.casualreview.actions

import com.casualreview.Comment
import com.casualreview.CrCli
import com.casualreview.CrService
import com.casualreview.CrSettings
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.progress.Task
import com.intellij.openapi.project.Project
import com.intellij.openapi.project.guessProjectDir
import com.intellij.openapi.ui.Messages
import java.io.File

private fun notify(project: Project, content: String, type: NotificationType) {
    NotificationGroupManager.getInstance()
        .getNotificationGroup("Casual Review")
        .createNotification(content, type)
        .notify(project)
}

private fun notifyError(project: Project, msg: String) =
    notify(project, msg, NotificationType.ERROR)

private fun notifyInfo(project: Project, msg: String) =
    notify(project, msg, NotificationType.INFORMATION)

class AddCommentAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val editor = e.getData(CommonDataKeys.EDITOR) ?: return
        val virtualFile = e.getData(CommonDataKeys.VIRTUAL_FILE) ?: return
        val cwd = project.guessProjectDir()?.path ?: return
        val rel = try {
            File(cwd).toPath().relativize(File(virtualFile.path).toPath()).toString()
        } catch (ex: Exception) {
            notifyError(project, "File is outside the project: ${virtualFile.path}")
            return
        }
        val sel = editor.selectionModel
        val startLine = editor.document.getLineNumber(sel.selectionStart) + 1
        val endLine = editor.document.getLineNumber(
            if (sel.hasSelection()) (sel.selectionEnd - 1).coerceAtLeast(sel.selectionStart) else sel.selectionStart
        ) + 1
        val lines = if (startLine == endLine) "$startLine" else "$startLine:$endLine"

        val body = Messages.showMultilineInputDialog(
            project,
            "Comment on $rel:$lines",
            "Add Comment",
            "",
            null,
            null,
        ) ?: return
        if (body.isBlank()) return

        runOnPool(project, "Adding comment") {
            val r = CrCli.run(project, listOf("comment", "add", rel, "--lines", lines, "-m", body))
            if (r.ok) {
                CrService.get(project).refreshNow()
                notifyInfo(project, "Comment added on $rel:$lines")
            } else {
                notifyError(project, "cr comment add failed: ${r.stderr.trim()}")
            }
        }
    }
}

class ReplyAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val target = pickRoot(project, "Reply to which comment?") ?: return
        replyToComment(project, target.id, target.author.name)
    }
}

class ResolveAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val target = pickRoot(project, "Resolve which comment?") ?: return
        resolveCommentById(project, target.id)
    }
}

/**
 * Inline reply action bound to a specific comment id at construction time.
 * Used by the gutter-icon popup menu so right-clicking a comment lets the
 * user reply without going through the global picker.
 */
class InlineReplyAction(
    private val project: Project,
    private val commentId: String,
    private val authorHint: String,
) : AnAction("Reply", "Reply to this comment", null) {
    override fun actionPerformed(e: AnActionEvent) {
        replyToComment(project, commentId, authorHint)
    }
}

class InlineResolveAction(
    private val project: Project,
    private val commentId: String,
) : AnAction("Resolve", "Mark this thread resolved", null) {
    override fun actionPerformed(e: AnActionEvent) {
        resolveCommentById(project, commentId)
    }
}

internal fun replyToComment(project: Project, commentId: String, authorHint: String) {
    val body = Messages.showMultilineInputDialog(
        project,
        "Reply to $commentId${if (authorHint.isNotBlank()) " ($authorHint)" else ""}",
        "Reply",
        "",
        null,
        null,
    ) ?: return
    if (body.isBlank()) return
    submitReply(project, commentId, body)
}

/**
 * Send a reply without prompting (used by the tool window's inline reply
 * box). Resolves the parent's origin commit so replies to comments
 * projected from ancestors land on the correct note.
 */
internal fun submitReply(project: Project, commentId: String, body: String) {
    val target = CrService.get(project).currentPayload()
        ?.comments?.firstOrNull { it.id == commentId }
    val commit = target?.originCommit
    runOnPool(project, "Replying") {
        val args = mutableListOf("comment", "reply", commentId, "-m", body)
        if (!commit.isNullOrBlank()) args += listOf("--commit", commit)
        val r = CrCli.run(project, args)
        if (r.ok) {
            CrService.get(project).refreshNow()
            notifyInfo(project, "Replied to $commentId")
        } else {
            notifyError(project, "cr comment reply failed: ${r.stderr.trim()}")
        }
    }
}

internal fun resolveCommentById(project: Project, commentId: String) {
    val message = Messages.showInputDialog(
        project,
        "Resolution message for $commentId (optional):",
        "Resolve",
        null,
    )
    val target = CrService.get(project).currentPayload()
        ?.comments?.firstOrNull { it.id == commentId }
    val commit = target?.originCommit
    runOnPool(project, "Resolving") {
        val args = mutableListOf("comment", "resolve", commentId)
        if (!message.isNullOrBlank()) args += listOf("-m", message)
        if (!commit.isNullOrBlank()) args += listOf("--commit", commit)
        val r = CrCli.run(project, args)
        if (r.ok) {
            CrService.get(project).refreshNow()
            notifyInfo(project, "Resolved $commentId")
        } else {
            notifyError(project, "cr comment resolve failed: ${r.stderr.trim()}")
        }
    }
}

class RefreshAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        CrService.get(project).refresh()
    }
}

class SyncAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val remote = CrSettings.get(project).remote
        runOnPool(project, "Syncing with $remote") {
            val fetch = CrCli.run(project, listOf("fetch", remote))
            if (!fetch.ok) {
                notifyError(project, "cr fetch failed: ${fetch.stderr.trim()}")
                return@runOnPool
            }
            val push = CrCli.run(project, listOf("push", remote))
            if (!push.ok) {
                notifyError(project, "cr push failed: ${push.stderr.trim()}")
                return@runOnPool
            }
            CrService.get(project).refreshNow()
            notifyInfo(project, "Synced with $remote")
        }
    }
}

class FetchAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val remote = CrSettings.get(project).remote
        runOnPool(project, "Fetching $remote") {
            val r = CrCli.run(project, listOf("fetch", remote))
            if (r.ok) {
                CrService.get(project).refreshNow()
                notifyInfo(project, "Fetched from $remote")
            } else {
                notifyError(project, "cr fetch failed: ${r.stderr.trim()}")
            }
        }
    }
}

class PushAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val remote = CrSettings.get(project).remote
        runOnPool(project, "Pushing $remote") {
            val r = CrCli.run(project, listOf("push", remote))
            if (r.ok) notifyInfo(project, "Pushed to $remote")
            else notifyError(project, "cr push failed: ${r.stderr.trim()}")
        }
    }
}

class ShowStaleAction : AnAction() {
    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return
        val service = CrService.get(project)
        val payload = service.currentPayload()
        val stale = service.staleIdsSnapshot()
        if (payload == null || stale.isEmpty()) {
            notifyInfo(project, "No stale comments.")
            return
        }
        val candidates = payload.comments.filter { it.id in stale }
        val labels = candidates.map { c ->
            val anchor = c.anchor.file?.let { "$it:${c.anchor.lineRange[0]}" } ?: "<commit>"
            "${c.id}  $anchor  — ${c.body.lineSequence().firstOrNull().orEmpty().take(80)}"
        }.toTypedArray()
        val choice = Messages.showEditableChooseDialog(
            "Stale comments",
            "Show Stale",
            null,
            labels,
            labels.firstOrNull(),
            null,
        ) ?: return
        val idx = labels.indexOf(choice)
        if (idx < 0) return
        val target = candidates[idx]
        openAt(project, target)
    }

    private fun openAt(project: Project, c: Comment) {
        val cwd = project.guessProjectDir()?.path ?: return
        val rel = c.anchor.file ?: return
        val vfs = com.intellij.openapi.vfs.LocalFileSystem.getInstance()
        val vf = vfs.findFileByPath(File(cwd, rel).absolutePath) ?: return
        val line = (c.anchor.lineRange.firstOrNull() ?: 1) - 1
        FileEditorManager.getInstance(project)
            .openTextEditor(
                com.intellij.openapi.fileEditor.OpenFileDescriptor(project, vf, line.coerceAtLeast(0), 0),
                true,
            )
    }
}

private fun pickRoot(project: Project, prompt: String): Comment? {
    val payload = CrService.get(project).currentPayload()
    if (payload == null || payload.comments.isEmpty()) {
        notifyInfo(project, "No comments to pick from. Try Refresh first.")
        return null
    }
    val resolvedRoots = payload.comments
        .filter { it.resolved && it.parent != null }
        .mapNotNull { it.parent }
        .toSet()
    val roots = payload.comments.filter { it.parent == null && it.id !in resolvedRoots }
    if (roots.isEmpty()) {
        notifyInfo(project, "No open threads.")
        return null
    }
    val labels = roots.map { c ->
        val anchor = c.anchor.file?.let { "$it:${c.anchor.lineRange[0]}" } ?: "<commit>"
        "${c.id}  $anchor  — ${c.body.lineSequence().firstOrNull().orEmpty().take(80)}"
    }.toTypedArray()
    val choice = Messages.showEditableChooseDialog(
        prompt, "Casual Review", null, labels, labels.first(), null,
    ) ?: return null
    val idx = labels.indexOf(choice)
    return if (idx >= 0) roots[idx] else null
}

private fun runOnPool(project: Project, title: String, block: () -> Unit) {
    ProgressManager.getInstance().run(object : Task.Backgroundable(project, title, false) {
        override fun run(indicator: ProgressIndicator) {
            block()
        }
    })
}
