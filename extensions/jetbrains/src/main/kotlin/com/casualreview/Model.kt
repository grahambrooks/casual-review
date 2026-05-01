package com.casualreview

import com.google.gson.annotations.SerializedName

/**
 * `casual-review/comment/1` schema mirror. Field names match the JSON
 * produced by `cr comment list --format json`.
 */
data class Author(
    val name: String = "",
    val email: String = "",
)

data class Anchor(
    val file: String? = null,
    @SerializedName("line_range") val lineRange: List<Int> = listOf(0, 0),
    @SerializedName("byte_range") val byteRange: List<Int> = listOf(0, 0),
    @SerializedName("anchor_text_sha") val anchorTextSha: String = "",
)

data class Comment(
    val id: String = "",
    val author: Author = Author(),
    @SerializedName("created_at") val createdAt: String = "",
    val anchor: Anchor = Anchor(),
    val body: String = "",
    val parent: String? = null,
    val resolved: Boolean = false,
    @SerializedName("origin_commit") val originCommit: String? = null,
)

data class CommentsPayload(
    val schema: String = "",
    val tool: String = "",
    @SerializedName("tool_version") val toolVersion: String = "",
    val commit: String = "",
    val comments: List<Comment> = emptyList(),
)
