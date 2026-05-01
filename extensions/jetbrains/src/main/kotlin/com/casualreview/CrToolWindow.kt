package com.casualreview

import com.casualreview.actions.replyToComment
import com.casualreview.actions.resolveCommentById
import com.casualreview.actions.submitReply
import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.fileEditor.FileEditorManagerEvent
import com.intellij.openapi.fileEditor.FileEditorManagerListener
import com.intellij.openapi.project.Project
import com.intellij.openapi.project.guessProjectDir
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.JBColor
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBScrollPane
import com.intellij.ui.components.JBTextArea
import com.intellij.util.ui.JBFont
import com.intellij.util.ui.JBUI
import java.awt.BorderLayout
import java.awt.Color
import java.awt.Component
import java.awt.Dimension
import java.awt.FlowLayout
import java.awt.event.KeyAdapter
import java.awt.event.KeyEvent
import java.io.File
import javax.swing.BorderFactory
import javax.swing.Box
import javax.swing.BoxLayout
import javax.swing.JButton
import javax.swing.JComponent
import javax.swing.JLabel
import javax.swing.JPanel
import javax.swing.SwingConstants

class CrToolWindowFactory : ToolWindowFactory {
    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val panel = CrCommentsPanel(project)
        val content = com.intellij.ui.content.ContentFactory.getInstance()
            .createContent(panel, "", false)
        Disposer.register(toolWindow.disposable, panel)
        toolWindow.contentManager.addContent(content)
    }
}

/**
 * Tool window showing comments anchored to the active editor's file. Each
 * thread is a card with the root comment, indented replies, and an inline
 * reply box. Updates live on `FileEditorManagerListener.selectionChanged`
 * and on the `CrEvents.TOPIC` message-bus topic.
 */
class CrCommentsPanel(private val project: Project) : JPanel(BorderLayout()), Disposable {

    private val list = JPanel().apply {
        layout = BoxLayout(this, BoxLayout.Y_AXIS)
        border = JBUI.Borders.empty(8)
    }
    private val scrollPane = JBScrollPane(list).apply {
        verticalScrollBar.unitIncrement = 16
        border = BorderFactory.createEmptyBorder()
    }
    private val emptyLabel = JBLabel(
        "<html><center>No comments for this file.<br/>" +
            "Tools → Casual Review → Add Comment on Selection</center></html>",
        SwingConstants.CENTER,
    ).apply {
        foreground = JBColor.GRAY
    }

    init {
        add(buildHeader(), BorderLayout.NORTH)
        add(scrollPane, BorderLayout.CENTER)

        // React to editor selection changes.
        project.messageBus.connect(this).subscribe(
            FileEditorManagerListener.FILE_EDITOR_MANAGER,
            object : FileEditorManagerListener {
                override fun selectionChanged(event: FileEditorManagerEvent) {
                    rerender()
                }

                override fun fileOpened(source: FileEditorManager, file: VirtualFile) {
                    rerender()
                }

                override fun fileClosed(source: FileEditorManager, file: VirtualFile) {
                    rerender()
                }
            },
        )

        // React to comment-payload changes.
        project.messageBus.connect(this).subscribe(
            CrEvents.TOPIC,
            CrCommentsListener { rerender() },
        )

        rerender()
    }

    private fun buildHeader(): JComponent {
        val header = JPanel(FlowLayout(FlowLayout.LEFT, 6, 4))
        header.border = BorderFactory.createMatteBorder(0, 0, 1, 0, JBColor.border())
        val title = JBLabel("Casual Review").apply { font = JBFont.label().asBold() }
        val refresh = JButton("Refresh").apply {
            addActionListener { CrService.get(project).refresh() }
        }
        val sync = JButton("Sync").apply {
            addActionListener {
                val remote = CrSettings.get(project).remote
                ApplicationManager.getApplication().executeOnPooledThread {
                    CrCli.run(project, listOf("fetch", remote))
                    CrCli.run(project, listOf("push", remote))
                    CrService.get(project).refreshNow()
                }
            }
        }
        header.add(title)
        header.add(Box.createHorizontalStrut(8))
        header.add(refresh)
        header.add(sync)
        return header
    }

    private fun rerender() {
        ApplicationManager.getApplication().invokeLater {
            list.removeAll()
            val file = activeFileRel()
            val payload = CrService.get(project).currentPayload()
            val visible = visibleThreads(payload, file)

            if (file == null) {
                list.add(centeredLabel("Open a file to view its comments."))
            } else if (visible.isEmpty()) {
                list.add(emptyLabel)
            } else {
                for (thread in visible) list.add(buildThreadCard(thread))
            }
            list.add(Box.createVerticalGlue())
            list.revalidate()
            list.repaint()
        }
    }

    private fun centeredLabel(text: String): JComponent =
        JBLabel(text, SwingConstants.CENTER).apply { foreground = JBColor.GRAY }

    private fun activeFileRel(): String? {
        val cwd = project.guessProjectDir()?.path ?: return null
        val vf = FileEditorManager.getInstance(project).selectedFiles.firstOrNull() ?: return null
        return try {
            File(cwd).toPath().relativize(File(vf.path).toPath()).toString()
        } catch (_: Exception) {
            null
        }
    }

    private data class Thread(val root: Comment, val replies: List<Comment>, val stale: Boolean)

    private fun visibleThreads(payload: CommentsPayload?, fileRel: String?): List<Thread> {
        if (payload == null || fileRel == null) return emptyList()
        val staleIds = CrService.get(project).staleIdsSnapshot()
        val byParent = payload.comments
            .filter { it.parent != null }
            .groupBy { it.parent!! }
        val resolvedRoots = payload.comments
            .filter { it.resolved && it.parent != null }
            .mapNotNull { it.parent }
            .toSet()

        return payload.comments
            .filter { it.parent == null }
            .filter { it.id !in resolvedRoots }
            .filter { c ->
                val anchorFile = c.anchor.file ?: return@filter false
                pathsEqual(anchorFile, fileRel)
            }
            .map { root ->
                val replies = byParent[root.id]?.sortedBy { it.createdAt } ?: emptyList()
                Thread(root, replies, root.id in staleIds)
            }
            .sortedBy { it.root.anchor.lineRange.firstOrNull() ?: 0 }
    }

    private fun pathsEqual(a: String, b: String): Boolean =
        a.replace('\\', '/').trim('/') == b.replace('\\', '/').trim('/')

    private fun buildThreadCard(thread: Thread): JComponent {
        val card = JPanel(BorderLayout()).apply {
            border = BorderFactory.createCompoundBorder(
                BorderFactory.createMatteBorder(1, 1, 1, 1, JBColor.border()),
                JBUI.Borders.empty(8),
            )
            background = JBColor.background()
            alignmentX = Component.LEFT_ALIGNMENT
            maximumSize = Dimension(Int.MAX_VALUE, Int.MAX_VALUE)
        }

        val body = JPanel().apply {
            layout = BoxLayout(this, BoxLayout.Y_AXIS)
            isOpaque = false
        }
        body.add(buildCommentBlock(thread.root, isReply = false, threadStale = thread.stale))
        for (reply in thread.replies) {
            body.add(Box.createVerticalStrut(6))
            body.add(buildCommentBlock(reply, isReply = true, threadStale = false))
        }
        body.add(Box.createVerticalStrut(8))
        body.add(buildReplyComposer(thread.root))

        card.add(body, BorderLayout.CENTER)

        val wrapper = JPanel().apply {
            layout = BoxLayout(this, BoxLayout.Y_AXIS)
            isOpaque = false
            alignmentX = Component.LEFT_ALIGNMENT
            add(card)
            add(Box.createVerticalStrut(8))
        }
        return wrapper
    }

    private fun buildCommentBlock(
        comment: Comment,
        isReply: Boolean,
        threadStale: Boolean,
    ): JComponent {
        val outer = JPanel(BorderLayout()).apply {
            isOpaque = false
            border = if (isReply) JBUI.Borders.emptyLeft(16) else JBUI.Borders.empty()
        }

        val header = JPanel(FlowLayout(FlowLayout.LEFT, 6, 0)).apply { isOpaque = false }
        val nameLabel = JLabel(comment.author.name.ifBlank { "<unknown>" }).apply {
            font = JBFont.label().asBold()
        }
        header.add(nameLabel)

        val anchor = comment.anchor.file?.let {
            val (a, b) = comment.anchor.lineRange.let {
                if (it.size >= 2) it[0] to it[1] else 0 to 0
            }
            if (a > 0) "$it:$a-$b" else it
        } ?: "<commit>"
        header.add(JLabel("· $anchor").apply { foreground = JBColor.GRAY })

        if (!isReply) {
            if (threadStale) header.add(taggedLabel("stale", JBColor.RED))
            comment.originCommit?.let {
                header.add(taggedLabel("from ${it.take(8)}", JBColor.GRAY))
            }
        }

        val resolveBtn = if (!isReply) {
            JButton("Resolve").apply {
                margin = JBUI.insets(0, 6)
                addActionListener { resolveCommentById(project, comment.id) }
            }
        } else null

        val headerRow = JPanel(BorderLayout()).apply { isOpaque = false }
        headerRow.add(header, BorderLayout.WEST)
        if (resolveBtn != null) headerRow.add(resolveBtn, BorderLayout.EAST)

        val bodyArea = JBTextArea(comment.body).apply {
            isEditable = false
            lineWrap = true
            wrapStyleWord = true
            isOpaque = false
            border = JBUI.Borders.emptyTop(4)
            font = JBFont.label()
        }

        outer.add(headerRow, BorderLayout.NORTH)
        outer.add(bodyArea, BorderLayout.CENTER)
        return outer
    }

    private fun taggedLabel(text: String, color: Color): JComponent {
        return JLabel("[$text]").apply {
            foreground = color
            font = JBFont.small()
        }
    }

    private fun buildReplyComposer(root: Comment): JComponent {
        val text = JBTextArea(2, 0).apply {
            lineWrap = true
            wrapStyleWord = true
            border = BorderFactory.createCompoundBorder(
                BorderFactory.createLineBorder(JBColor.border()),
                JBUI.Borders.empty(4),
            )
        }

        val send = JButton("Send").apply {
            margin = JBUI.insets(0, 8)
        }

        val submit = {
            val body = text.text.trim()
            if (body.isNotEmpty()) {
                submitReply(project, root.id, body)
                text.text = ""
            }
        }
        send.addActionListener { submit() }

        // Cmd/Ctrl+Enter to send without taking the mouse.
        text.addKeyListener(object : KeyAdapter() {
            override fun keyPressed(e: KeyEvent) {
                val mod = e.modifiersEx
                val cmdOrCtrl = (mod and KeyEvent.META_DOWN_MASK) != 0 ||
                    (mod and KeyEvent.CTRL_DOWN_MASK) != 0
                if (e.keyCode == KeyEvent.VK_ENTER && cmdOrCtrl) {
                    e.consume()
                    submit()
                }
            }
        })

        val composer = JPanel(BorderLayout(6, 4)).apply {
            isOpaque = false
            border = JBUI.Borders.emptyTop(4)
        }
        composer.add(text, BorderLayout.CENTER)

        val sendRow = JPanel(BorderLayout()).apply {
            isOpaque = false
            add(JLabel("Reply (⌘/Ctrl+Enter)").apply {
                foreground = JBColor.GRAY
                font = JBFont.small()
            }, BorderLayout.WEST)
            add(send, BorderLayout.EAST)
        }
        composer.add(sendRow, BorderLayout.SOUTH)
        return composer
    }

    @Suppress("unused")
    private fun openInDialog(commentId: String, authorHint: String) {
        replyToComment(project, commentId, authorHint)
    }

    override fun dispose() {
        // MessageBus connection is bound to `this` via `connect(this)` so it
        // unsubscribes automatically. Nothing else to clean up.
    }
}
