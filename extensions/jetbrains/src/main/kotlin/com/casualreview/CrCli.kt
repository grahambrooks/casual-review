package com.casualreview

import com.google.gson.Gson
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.project.guessProjectDir
import java.io.File
import java.security.MessageDigest

/**
 * Synchronous shell-out wrapper around the `cr` binary. Every operation is
 * one process invocation; no daemon, no IPC.
 */
object CrCli {
    private val log = Logger.getInstance(CrCli::class.java)
    private val gson = Gson()

    data class Result(val exitCode: Int, val stdout: String, val stderr: String) {
        val ok: Boolean get() = exitCode == 0
    }

    fun run(project: Project, args: List<String>, stdin: String? = null): Result {
        val cwd = project.guessProjectDir()?.path
            ?: error("project has no base directory")
        val bin = CrSettings.get(project).binPath
        val cmd = mutableListOf(bin) + args

        log.info("$ ${cmd.joinToString(" ")}")
        val builder = ProcessBuilder(cmd)
            .directory(File(cwd))
            .redirectErrorStream(false)
        val process = builder.start()

        if (stdin != null) {
            process.outputStream.use { it.write(stdin.toByteArray()) }
        } else {
            process.outputStream.close()
        }

        val stdout = process.inputStream.bufferedReader().readText()
        val stderr = process.errorStream.bufferedReader().readText()
        process.waitFor()
        if (stderr.isNotBlank()) log.info("stderr: ${stderr.trim()}")
        return Result(process.exitValue(), stdout, stderr)
    }

    fun listComments(project: Project): CommentsPayload? {
        val args = mutableListOf("comment", "list", "--format", "json")
        if (CrSettings.get(project).includeAncestors) args += "--include-ancestors"
        val r = run(project, args)
        if (!r.ok) {
            log.warn("cr comment list failed: ${r.stderr.trim()}")
            return null
        }
        if (r.stdout.isBlank()) return null
        return try {
            gson.fromJson(r.stdout, CommentsPayload::class.java)
        } catch (e: Exception) {
            log.warn("failed to parse cr output: ${e.message}")
            null
        }
    }

    fun sha256Hex(bytes: ByteArray): String {
        val md = MessageDigest.getInstance("SHA-256")
        val digest = md.digest(bytes)
        val sb = StringBuilder(digest.size * 2)
        for (b in digest) {
            val v = b.toInt() and 0xff
            sb.append("0123456789abcdef"[v ushr 4])
            sb.append("0123456789abcdef"[v and 0x0f])
        }
        return sb.toString()
    }
}
