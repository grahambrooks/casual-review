package com.casualreview

import com.intellij.util.messages.Topic

/**
 * MessageBus topic published by CrService whenever the comment payload
 * changes (initial load, refresh, mutation success). UI components
 * (tool window, status bar) subscribe to redraw without polling.
 */
fun interface CrCommentsListener {
    fun commentsChanged()
}

object CrEvents {
    val TOPIC: Topic<CrCommentsListener> =
        Topic.create("CasualReviewComments", CrCommentsListener::class.java)
}
