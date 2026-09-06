package com.casualreview

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
import com.intellij.ui.components.JBCheckBox
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBScrollPane
import com.intellij.ui.components.JBTextArea
import com.intellij.util.ui.JBFont
import com.intellij.util.ui.JBUI
import java.awt.BorderLayout
import java.awt.Color
import java.awt.Component
import java.awt.Cursor
import java.awt.Dimension
import java.awt.FlowLayout
import java.awt.event.KeyAdapter
import java.awt.event.KeyEvent
import java.awt.event.MouseAdapter
import java.awt.event.MouseEvent
import java.io.File
import java.time.Duration
import java.time.Instant
import java.time.OffsetDateTime
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
 * thread is a compact card: a root comment, a threaded run of replies behind a
 * left rule, and a reply box that stays collapsed until you ask for it. Cards
 * are height-clamped so they hug their content instead of stretching to fill
 * the panel. Updates live on `FileEditorManagerListener.selectionChanged` and
 * on the `CrEvents.TOPIC` message-bus topic.
 */
class CrCommentsPanel(private val project: Project) : JPanel(BorderLayout()), Disposable {

    /** Reply chains longer than this collapse behind a "show N replies" link. */
    private val collapseRepliesOver = 3

    private val list = JPanel().apply {
        layout = BoxLayout(this, BoxLayout.Y_AXIS)
        border = JBUI.Borders.empty(6)
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

    /** When true, show every thread in the project grouped by file. */
    private var showAllFiles: Boolean = false

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
        val allFiles = JBCheckBox("All files", showAllFiles).apply {
            toolTipText = "Show every thread in the project, grouped by file"
            addActionListener {
                showAllFiles = isSelected
                rerender()
            }
        }
        header.add(title)
        header.add(Box.createHorizontalStrut(8))
        header.add(refresh)
        header.add(sync)
        header.add(allFiles)
        return header
    }

    private fun rerender() {
        ApplicationManager.getApplication().invokeLater {
            list.removeAll()
            val payload = CrService.get(project).currentPayload()

            if (showAllFiles) {
                renderAllFiles(payload)
            } else {
                renderActiveFile(payload)
            }

            list.add(Box.createVerticalGlue())
            list.revalidate()
            list.repaint()
        }
    }

    private fun renderActiveFile(payload: CommentsPayload?) {
        val file = activeFileRel()
        val visible = visibleThreads(payload, file)
        if (file == null) {
            list.add(centeredLabel("Open a file to view its comments."))
        } else if (visible.isEmpty()) {
            list.add(emptyLabel)
        } else {
            for (thread in visible) list.add(buildThreadCard(thread))
        }
    }

    private fun renderAllFiles(payload: CommentsPayload?) {
        val threads = allVisibleThreads(payload)
        if (threads.isEmpty()) {
            list.add(centeredLabel("No open comments in this project."))
            return
        }
        var currentFile: String? = null
        for (thread in threads) {
            val file = thread.root.anchor.file ?: "<commit>"
            if (file != currentFile) {
                if (currentFile != null) list.add(Box.createVerticalStrut(6))
                list.add(fileHeader(file))
                currentFile = file
            }
            list.add(buildThreadCard(thread))
        }
    }

    private fun fileHeader(file: String): JComponent =
        JBLabel(file).apply {
            font = JBFont.label().asBold()
            foreground = JBColor.GRAY
            alignmentX = Component.LEFT_ALIGNMENT
            border = JBUI.Borders.empty(4, 2)
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

    /** Every open, file-anchored thread in the project, grouped-friendly: sorted by file then line. */
    private fun allVisibleThreads(payload: CommentsPayload?): List<Thread> {
        if (payload == null) return emptyList()
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
            .filter { it.anchor.file != null }
            .map { root ->
                val replies = byParent[root.id]?.sortedBy { it.createdAt } ?: emptyList()
                Thread(root, replies, root.id in staleIds)
            }
            .sortedWith(
                compareBy(
                    { it.root.anchor.file?.replace('\\', '/')?.trim('/') ?: "" },
                    { it.root.anchor.lineRange.firstOrNull() ?: 0 },
                ),
            )
    }

    private fun pathsEqual(a: String, b: String): Boolean =
        a.replace('\\', '/').trim('/') == b.replace('\\', '/').trim('/')

    /**
     * A vertical container whose maximum height equals its preferred height, so
     * it never stretches under a Y-axis BoxLayout. This is what keeps cards
     * hugging their content instead of splitting the panel's free space.
     */
    private class ContentBox : JPanel() {
        init {
            layout = BoxLayout(this, BoxLayout.Y_AXIS)
            isOpaque = false
            alignmentX = Component.LEFT_ALIGNMENT
        }

        override fun getMaximumSize(): Dimension =
            Dimension(Int.MAX_VALUE, preferredSize.height)
    }

    private fun buildThreadCard(thread: Thread): JComponent {
        val body = ContentBox()

        val composerText = JBTextArea(2, 0).apply {
            lineWrap = true
            wrapStyleWord = true
            border = BorderFactory.createCompoundBorder(
                BorderFactory.createLineBorder(JBColor.border()),
                JBUI.Borders.empty(4),
            )
        }
        val composer = buildReplyComposer(thread.root, composerText).apply { isVisible = false }
        val repliesBox = JPanel().apply {
            layout = BoxLayout(this, BoxLayout.Y_AXIS)
            isOpaque = false
            alignmentX = Component.LEFT_ALIGNMENT
        }

        fun relayout() {
            list.revalidate()
            list.repaint()
        }

        fun renderReplies(expanded: Boolean) {
            repliesBox.removeAll()
            if (thread.replies.isNotEmpty()) {
                repliesBox.add(Box.createVerticalStrut(6))
                if (!expanded) {
                    val n = thread.replies.size
                    repliesBox.add(
                        linkLabel("▸ show $n ${if (n == 1) "reply" else "replies"}") {
                            renderReplies(true); relayout()
                        },
                    )
                } else {
                    if (thread.replies.size > collapseRepliesOver) {
                        repliesBox.add(
                            linkLabel("▾ hide replies") { renderReplies(false); relayout() },
                        )
                        repliesBox.add(Box.createVerticalStrut(4))
                    }
                    thread.replies.forEachIndexed { i, reply ->
                        if (i > 0) repliesBox.add(Box.createVerticalStrut(4))
                        repliesBox.add(buildReplyBlock(reply))
                    }
                }
            }
            repliesBox.revalidate()
            repliesBox.repaint()
        }

        val rootBlock = buildRootBlock(thread) {
            composer.isVisible = !composer.isVisible
            relayout()
            if (composer.isVisible) composerText.requestFocusInWindow()
        }

        body.add(rootBlock)
        body.add(repliesBox)
        body.add(composer)
        renderReplies(thread.replies.size <= collapseRepliesOver)

        val card = JPanel(BorderLayout()).apply {
            border = BorderFactory.createCompoundBorder(
                BorderFactory.createMatteBorder(1, 1, 1, 1, JBColor.border()),
                JBUI.Borders.empty(6, 8),
            )
            background = JBColor.background()
            alignmentX = Component.LEFT_ALIGNMENT
            add(body, BorderLayout.CENTER)
        }

        return ContentBox().apply {
            add(card)
            add(Box.createVerticalStrut(6))
        }
    }

    /** Root comment: meta line with Reply/Resolve actions, then the wrapped body. */
    private fun buildRootBlock(thread: Thread, onReplyToggle: () -> Unit): JComponent {
        val comment = thread.root
        val outer = JPanel(BorderLayout()).apply { isOpaque = false }

        val meta = metaRow(comment).apply {
            if (thread.stale) add(taggedLabel("stale", JBColor.RED))
            comment.originCommit?.let { add(taggedLabel("from ${it.take(8)}", JBColor.GRAY)) }
        }

        val actions = JPanel(FlowLayout(FlowLayout.RIGHT, 8, 0)).apply { isOpaque = false }
        actions.add(linkLabel("Reply") { onReplyToggle() })
        actions.add(linkLabel("Resolve") { resolveCommentById(project, comment.id) })

        val headerRow = JPanel(BorderLayout()).apply { isOpaque = false }
        headerRow.add(meta, BorderLayout.WEST)
        headerRow.add(actions, BorderLayout.EAST)

        outer.add(headerRow, BorderLayout.NORTH)
        outer.add(bodyArea(comment.body), BorderLayout.CENTER)
        return outer
    }

    /** Reply comment: indented behind a left thread rule, compact meta, body. */
    private fun buildReplyBlock(comment: Comment): JComponent {
        val inner = JPanel(BorderLayout()).apply { isOpaque = false }
        inner.add(metaRow(comment), BorderLayout.NORTH)
        inner.add(bodyArea(comment.body), BorderLayout.CENTER)

        return JPanel(BorderLayout()).apply {
            isOpaque = false
            alignmentX = Component.LEFT_ALIGNMENT
            border = BorderFactory.createCompoundBorder(
                JBUI.Borders.emptyLeft(2),
                BorderFactory.createCompoundBorder(
                    BorderFactory.createMatteBorder(0, 2, 0, 0, JBColor.border()),
                    JBUI.Borders.emptyLeft(8),
                ),
            )
            add(inner, BorderLayout.CENTER)
        }
    }

    /** `Author · file:line · 3h` on a single tight row. */
    private fun metaRow(comment: Comment): JPanel {
        val row = JPanel(FlowLayout(FlowLayout.LEFT, 6, 0)).apply { isOpaque = false }
        row.add(
            JLabel(comment.author.name.ifBlank { "<unknown>" }).apply { font = JBFont.label().asBold() },
        )
        val anchor = comment.anchor.file?.let { file ->
            val range = comment.anchor.lineRange
            val a = range.getOrNull(0) ?: 0
            val b = range.getOrNull(1) ?: 0
            if (a > 0) "$file:$a-$b" else file
        } ?: "<commit>"
        row.add(JLabel("· $anchor").apply { foreground = JBColor.GRAY; font = JBFont.small() })
        relativeTime(comment.createdAt).takeIf { it.isNotEmpty() }?.let {
            row.add(JLabel("· $it").apply { foreground = JBColor.GRAY; font = JBFont.small() })
        }
        return row
    }

    private fun bodyArea(text: String): JComponent =
        JBTextArea(text).apply {
            isEditable = false
            lineWrap = true
            wrapStyleWord = true
            isOpaque = false
            border = JBUI.Borders.emptyTop(3)
            font = JBFont.label()
            alignmentX = Component.LEFT_ALIGNMENT
        }

    private fun taggedLabel(text: String, color: Color): JComponent =
        JLabel("[$text]").apply {
            foreground = color
            font = JBFont.small()
        }

    /** A clickable, link-styled label that sits flush-left in a BoxLayout. */
    private fun linkLabel(text: String, onClick: () -> Unit): JLabel =
        JLabel(text).apply {
            foreground = JBColor.namedColor("Link.activeForeground", JBColor(0x589DF6, 0x548AF7))
            font = JBFont.small()
            alignmentX = Component.LEFT_ALIGNMENT
            cursor = Cursor.getPredefinedCursor(Cursor.HAND_CURSOR)
            addMouseListener(object : MouseAdapter() {
                override fun mouseClicked(e: MouseEvent) = onClick()
            })
        }

    private fun buildReplyComposer(root: Comment, text: JBTextArea): JComponent {
        val send = JButton("Send").apply { margin = JBUI.insets(0, 8) }

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
            alignmentX = Component.LEFT_ALIGNMENT
            border = JBUI.Borders.emptyTop(6)
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

    /** Compact age like `just now`, `5m`, `3h`, `2d`, or a date for older items. */
    private fun relativeTime(iso: String): String {
        if (iso.isBlank()) return ""
        return try {
            val then = OffsetDateTime.parse(iso)
            val secs = Duration.between(then.toInstant(), Instant.now()).seconds
            when {
                secs < 60 -> "just now"
                secs < 3600 -> "${secs / 60}m"
                secs < 86_400 -> "${secs / 3600}h"
                secs < 604_800 -> "${secs / 86_400}d"
                else -> then.toLocalDate().toString()
            }
        } catch (_: Exception) {
            ""
        }
    }

    override fun dispose() {
        // MessageBus connection is bound to `this` via `connect(this)` so it
        // unsubscribes automatically. Nothing else to clean up.
    }
}
