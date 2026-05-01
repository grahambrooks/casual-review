package com.casualreview

import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.StatusBar
import com.intellij.openapi.wm.StatusBarWidget
import com.intellij.openapi.wm.StatusBarWidgetFactory
import com.intellij.util.Consumer
import java.awt.event.MouseEvent
import javax.swing.Icon

class CrStatusBarWidgetFactory : StatusBarWidgetFactory {
    override fun getId(): String = "CasualReviewStatusBar"
    override fun getDisplayName(): String = "Casual Review"
    override fun isAvailable(project: Project): Boolean = true
    override fun createWidget(project: Project): StatusBarWidget = CrStatusBarWidget(project)
    override fun disposeWidget(widget: StatusBarWidget) {}
    override fun canBeEnabledOn(statusBar: StatusBar): Boolean = true
}

class CrStatusBarWidget(private val project: Project) :
    StatusBarWidget,
    StatusBarWidget.TextPresentation {

    private var statusBar: StatusBar? = null

    override fun ID(): String = "CasualReviewStatusBar"
    override fun getPresentation(): StatusBarWidget.WidgetPresentation = this

    override fun install(statusBar: StatusBar) {
        this.statusBar = statusBar
    }

    override fun dispose() {
        statusBar = null
    }

    override fun getText(): String {
        val service = CrService.get(project)
        val open = service.openThreadCount()
        val stale = service.staleCount()
        return if (stale > 0) "cr: $open ⚠$stale" else "cr: $open"
    }

    override fun getAlignment(): Float = 0f

    override fun getTooltipText(): String {
        val service = CrService.get(project)
        val open = service.openThreadCount()
        val stale = service.staleCount()
        return "Casual Review: $open open thread(s)" + if (stale > 0) ", $stale stale" else ""
    }

    override fun getClickConsumer(): Consumer<MouseEvent>? = Consumer {
        CrService.get(project).refresh()
    }
}
