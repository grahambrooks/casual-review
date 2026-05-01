package com.casualreview

import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.openapi.components.service
import com.intellij.openapi.options.Configurable
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.DialogPanel
import com.intellij.ui.dsl.builder.bindSelected
import com.intellij.ui.dsl.builder.bindText
import com.intellij.ui.dsl.builder.panel
import javax.swing.JComponent

@Service(Service.Level.PROJECT)
@State(name = "CasualReviewSettings", storages = [Storage("casual-review.xml")])
class CrSettings : PersistentStateComponent<CrSettings.State> {
    data class State(
        var binPath: String = "cr",
        var includeAncestors: Boolean = true,
        var remote: String = "origin",
    )

    private var myState = State()

    override fun getState(): State = myState

    override fun loadState(state: State) {
        myState = state
    }

    val binPath: String get() = myState.binPath.ifBlank { "cr" }
    val includeAncestors: Boolean get() = myState.includeAncestors
    val remote: String get() = myState.remote.ifBlank { "origin" }

    companion object {
        fun get(project: Project): CrSettings = project.service()
    }
}

class CrConfigurable(private val project: Project) : Configurable {
    private var settingsPanel: DialogPanel? = null

    override fun getDisplayName(): String = "Casual Review"

    override fun createComponent(): JComponent {
        val state = CrSettings.get(project).state
        val p = panel {
            row("cr binary path:") {
                textField()
                    .bindText({ state.binPath }, { state.binPath = it })
                    .comment("Absolute path or executable name on PATH. Default: cr")
            }
            row("Default remote:") {
                textField()
                    .bindText({ state.remote }, { state.remote = it })
                    .comment("Used by Sync / Fetch / Push.")
            }
            row {
                checkBox("Include ancestor commits when listing comments")
                    .bindSelected({ state.includeAncestors }, { state.includeAncestors = it })
                    .comment("Passes --include-ancestors to cr comment list.")
            }
        }
        settingsPanel = p
        return p
    }

    override fun isModified(): Boolean = settingsPanel?.isModified() ?: false

    override fun apply() {
        settingsPanel?.apply()
        CrService.get(project).refresh()
    }

    override fun reset() {
        settingsPanel?.reset()
    }

    override fun disposeUIResources() {
        settingsPanel = null
    }
}
