package com.casualreview

import com.intellij.icons.AllIcons
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.editor.Editor
import com.intellij.openapi.editor.ex.EditorEx
import com.intellij.openapi.editor.markup.GutterIconRenderer
import com.intellij.openapi.editor.markup.HighlighterLayer
import com.intellij.openapi.editor.markup.HighlighterTargetArea
import com.intellij.openapi.editor.markup.RangeHighlighter
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.Task
import com.intellij.openapi.project.Project
import com.intellij.openapi.project.guessProjectDir
import com.intellij.openapi.startup.ProjectActivity
import com.intellij.openapi.vfs.LocalFileSystem
import com.intellij.openapi.wm.WindowManager
import java.io.File
import javax.swing.Icon

/**
 * Project-scoped state holder. Caches the latest CommentsPayload, computes
 * staleness against the working tree, and (re)applies gutter decorations on
 * all open editors.
 */
@Service(Service.Level.PROJECT)
class CrService(private val project: Project) {
    @Volatile
    private var payload: CommentsPayload? = null

    @Volatile
    private var staleIds: Set<String> = emptySet()

    private val highlightersByEditor = mutableMapOf<Editor, MutableList<RangeHighlighter>>()

    fun currentPayload(): CommentsPayload? = payload
    fun staleIdsSnapshot(): Set<String> = staleIds

    /** Refresh asynchronously: shell out, parse, recompute staleness, redraw. */
    fun refresh() {
        ApplicationManager.getApplication().executeOnPooledThread {
            val newPayload = CrCli.listComments(project)
            val newStale = computeStaleness(newPayload)
            payload = newPayload
            staleIds = newStale
            ApplicationManager.getApplication().invokeLater {
                redrawAllEditors()
                refreshStatusBar()
                publishCommentsChanged()
            }
        }
    }

    /** Synchronous variant used after mutating commands so the redraw is
     *  visible by the time the user sees the next prompt. */
    fun refreshNow() {
        val newPayload = CrCli.listComments(project)
        val newStale = computeStaleness(newPayload)
        payload = newPayload
        staleIds = newStale
        ApplicationManager.getApplication().invokeLater {
            redrawAllEditors()
            refreshStatusBar()
            publishCommentsChanged()
        }
    }

    private fun publishCommentsChanged() {
        if (project.isDisposed) return
        try {
            project.messageBus.syncPublisher(CrEvents.TOPIC).commentsChanged()
        } catch (_: Throwable) {
            // Project closing — bus might be unavailable; ignore.
        }
    }

    private fun computeStaleness(p: CommentsPayload?): Set<String> {
        if (p == null) return emptySet()
        val cwd = project.guessProjectDir()?.path ?: return emptySet()
        val stale = mutableSetOf<String>()
        for (c in p.comments) {
            val rel = c.anchor.file ?: continue
            if (c.anchor.anchorTextSha.isBlank()) continue
            val f = File(cwd, rel)
            if (!f.exists()) {
                stale += c.id
                continue
            }
            val bytes = try {
                f.readBytes()
            } catch (e: Exception) {
                stale += c.id
                continue
            }
            val (lo, hi) = if (c.anchor.lineRange.size >= 2 &&
                c.anchor.lineRange[0] == 0 && c.anchor.lineRange[1] == 0
            ) {
                0 to bytes.size
            } else {
                val a = c.anchor.byteRange.getOrNull(0) ?: 0
                val b = c.anchor.byteRange.getOrNull(1) ?: 0
                a to b
            }
            if (lo < 0 || hi > bytes.size || lo > hi) {
                stale += c.id
                continue
            }
            val slice = bytes.copyOfRange(lo, hi)
            val sha = CrCli.sha256Hex(slice)
            if (sha != c.anchor.anchorTextSha) stale += c.id
        }
        return stale
    }

    fun redrawAllEditors() {
        val fem = FileEditorManager.getInstance(project)
        for (file in fem.openFiles) {
            for (fileEditor in fem.getEditors(file)) {
                val ed = (fileEditor as? com.intellij.openapi.fileEditor.TextEditor)?.editor
                    ?: continue
                applyDecorations(ed)
            }
        }
    }

    fun applyDecorations(editor: Editor) {
        clearDecorations(editor)
        val p = payload ?: return
        val cwd = project.guessProjectDir()?.path ?: return
        val file = editor.virtualFile ?: return
        val relPath = try {
            File(cwd).toPath().relativize(File(file.path).toPath()).toString()
        } catch (e: Exception) {
            return
        }

        val resolvedRoots = p.comments
            .filter { it.resolved && it.parent != null }
            .mapNotNull { it.parent }
            .toSet()

        val repliesByParent = p.comments
            .filter { it.parent != null }
            .groupBy { it.parent!! }

        val markup = (editor as? EditorEx)?.markupModel ?: editor.markupModel
        val list = highlightersByEditor.getOrPut(editor) { mutableListOf() }

        for (c in p.comments) {
            if (c.parent != null) continue
            if (c.id in resolvedRoots) continue
            val anchorFile = c.anchor.file ?: continue
            if (!pathsEqual(anchorFile, relPath)) continue
            val (start, end) = c.anchor.lineRange.let { if (it.size >= 2) it[0] to it[1] else 0 to 0 }
            if (start <= 0 || end <= 0) continue

            val lineCount = editor.document.lineCount
            val lineIdx = (start - 1).coerceAtMost((lineCount - 1).coerceAtLeast(0))
            val isStale = c.id in staleIds
            val replies = repliesByParent[c.id]?.filterNot { it.resolved }.orEmpty()

            val hl = markup.addLineHighlighter(lineIdx, HighlighterLayer.WARNING, null)
            hl.gutterIconRenderer = CommentGutterRenderer(project, c, replies, isStale)
            hl.setErrorStripeMarkColor(
                if (isStale) java.awt.Color(220, 80, 80) else java.awt.Color(255, 165, 0)
            )
            hl.setErrorStripeTooltip(renderTooltip(c, replies, isStale))
            list += hl
        }
    }

    private fun clearDecorations(editor: Editor) {
        val list = highlightersByEditor.remove(editor) ?: return
        val markup = (editor as? EditorEx)?.markupModel ?: editor.markupModel
        for (h in list) {
            try {
                markup.removeHighlighter(h)
            } catch (_: Exception) { /* editor disposed */ }
        }
    }

    private fun pathsEqual(a: String, b: String): Boolean =
        a.replace('\\', '/').trim('/') == b.replace('\\', '/').trim('/')

    private fun refreshStatusBar() {
        val sb = WindowManager.getInstance().getStatusBar(project) ?: return
        sb.updateWidget("CasualReviewStatusBar")
    }

    fun openThreadCount(): Int {
        val p = payload ?: return 0
        val resolvedRoots = p.comments
            .filter { it.resolved && it.parent != null }
            .mapNotNull { it.parent }
            .toSet()
        return p.comments.count { it.parent == null && it.id !in resolvedRoots }
    }

    fun staleCount(): Int = staleIds.size

    companion object {
        fun get(project: Project): CrService = project.service()
    }
}

/**
 * Auto-refresh on project open.
 */
class CrStartupActivity : ProjectActivity {
    override suspend fun execute(project: Project) {
        CrService.get(project).refresh()
    }
}

private fun renderTooltip(c: Comment, replies: List<Comment>, stale: Boolean): String {
    val sb = StringBuilder()
    sb.append("<html><b>")
    sb.append(escape(c.author.name))
    sb.append("</b> · <code>").append(c.id).append("</code>")
    if (stale) sb.append(" · <i>stale</i>")
    if (c.originCommit != null) sb.append(" · <i>from ").append(c.originCommit.take(8)).append("</i>")
    sb.append("<br/>").append(escape(c.body).replace("\n", "<br/>"))
    for (r in replies) {
        sb.append("<hr/><b>").append(escape(r.author.name)).append("</b>")
        sb.append(" · <code>").append(r.id).append("</code><br/>")
        sb.append(escape(r.body).replace("\n", "<br/>"))
    }
    sb.append("</html>")
    return sb.toString()
}

private fun escape(s: String): String = s
    .replace("&", "&amp;")
    .replace("<", "&lt;")
    .replace(">", "&gt;")

private class CommentGutterRenderer(
    private val project: Project,
    private val comment: Comment,
    private val replies: List<Comment>,
    private val stale: Boolean,
) : GutterIconRenderer() {
    override fun getIcon(): Icon = if (stale) AllIcons.General.Warning else AllIcons.General.Note
    override fun getTooltipText(): String = renderTooltip(comment, replies, stale)
    override fun isNavigateAction(): Boolean = true

    /** Left-click on the gutter icon opens the reply prompt for this comment. */
    override fun getClickAction(): com.intellij.openapi.actionSystem.AnAction =
        com.casualreview.actions.InlineReplyAction(project, comment.id, comment.author.name)

    /** Right-click shows Reply + Resolve scoped to this comment. */
    override fun getPopupMenuActions(): com.intellij.openapi.actionSystem.ActionGroup {
        val group = com.intellij.openapi.actionSystem.DefaultActionGroup()
        group.add(com.casualreview.actions.InlineReplyAction(project, comment.id, comment.author.name))
        group.add(com.casualreview.actions.InlineResolveAction(project, comment.id))
        return group
    }

    override fun getAlignment(): Alignment = Alignment.RIGHT

    override fun equals(other: Any?): Boolean =
        other is CommentGutterRenderer && other.comment.id == comment.id && other.stale == stale

    override fun hashCode(): Int = comment.id.hashCode() xor stale.hashCode()
}
