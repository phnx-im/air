// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.core.content.IntentCompat
import androidx.core.content.pm.ShortcutManagerCompat
import io.flutter.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.InputStream
import java.util.UUID

// Receives the system share sheet intents, extracts the shared content into
// the app cache and hands it to the main app.
class ShareActivity : ComponentActivity() {
    companion object {
        private const val TAG = "ShareActivity"
        const val SHARE_CACHE_DIR = "share"

        // Anything beyond is reported as dropped.
        private const val MAX_ATTACHMENTS = 10

        // Upper bound for copying a single shared stream.
        //
        // See `max_attachment_size` in StorageSettings in the backend).
        private const val MAX_ATTACHMENT_COPY_BYTES = 32L * 1024 * 1024

        // Cache entries older than this are leftovers of a killed share or
        // an abandoned handoff and are removed on the next share.
        private const val STALE_CACHE_AGE_MILLIS = 24L * 60 * 60 * 1000
    }

    private val scope = MainScope()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        val chatId = intent.getStringExtra(ShortcutManagerCompat.EXTRA_SHORTCUT_ID)
        val uris: List<Uri> = when (intent.action) {
            Intent.ACTION_SEND ->
                listOfNotNull(
                    IntentCompat.getParcelableExtra(
                        intent,
                        Intent.EXTRA_STREAM,
                        Uri::class.java
                    )
                )

            Intent.ACTION_SEND_MULTIPLE ->
                IntentCompat.getParcelableArrayListExtra(
                    intent,
                    Intent.EXTRA_STREAM,
                    Uri::class.java
                ).orEmpty()

            else -> emptyList()
        }

        if (text.isNullOrEmpty() && uris.isEmpty()) {
            openMainApp()
            finish()
            return
        }

        // Content URIs have no filesystem path behind them, so they are
        // copied into the app cache.
        scope.launch {
            val candidates = uris.take(MAX_ATTACHMENTS)
            val attachments = withContext(Dispatchers.IO) {
                deleteStaleCacheDirs()
                candidates.mapNotNull { uri -> copyToCache(uri) }
            }
            handOff(
                chatId = chatId,
                text = text,
                attachments = attachments,
                // Unreadable and oversized streams are skipped in the copy,
                // anything over the cap never reaches it.
                dropped = uris.size - attachments.size,
            )
            finish()
        }
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    // Hands the extracted content to the main app. The files belong to the
    // main app from here on, which deletes them after the upload.
    private fun handOff(
        chatId: String?,
        text: String?,
        attachments: List<Pair<String, String?>>,
        dropped: Int
    ) {
        PendingShare.put(
            mapOf(
                // Absent when the user picks the destination from the chat list
                "chatId" to chatId,
                "paths" to attachments.map { it.first },
                // Parallel to the paths; empty when the provider reported no type
                "mimeTypes" to attachments.map { it.second.orEmpty() },
                "text" to text,
                "dropped" to dropped,
            )
        )
        val intent = Intent(this, MainActivity::class.java).apply {
            action = MainActivity.SHARE_INTO_CHAT
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        startActivity(intent)
    }

    // Removes cache directories left behind by share sessions or handoffs
    // that never reached their upload.
    private fun deleteStaleCacheDirs() {
        try {
            val cutoff = System.currentTimeMillis() - STALE_CACHE_AGE_MILLIS
            File(cacheDir, SHARE_CACHE_DIR).listFiles()
                ?.filter { it.lastModified() < cutoff }
                ?.forEach { it.deleteRecursively() }
        } catch (e: Exception) {
            Log.w(TAG, "Failed to delete stale share cache directories", e)
        }
    }

    // Whether the URI is one another app may legitimately share with us.
    private fun isForeignContentUri(uri: Uri): Boolean =
        uri.scheme == ContentResolver.SCHEME_CONTENT &&
            uri.authority?.let { it == packageName || it.startsWith("$packageName.") } == false

    // Returns the `(path, mimeType)` entry for the handoff, or null when the
    // URI cannot be read.
    private fun copyToCache(uri: Uri): Pair<String, String?>? {
        if (!isForeignContentUri(uri)) {
            Log.w(TAG, "Dropping shared URI with scheme '${uri.scheme}'")
            return null
        }
        return try {
            val displayName = queryDisplayName(uri) ?: "shared"
            // The display name is attacker-controlled, so keep only the
            // last path segment.
            val fileName = File(displayName).name.ifEmpty { "shared" }
            val targetDir = File(cacheDir, "$SHARE_CACHE_DIR/${UUID.randomUUID()}")
            if (!targetDir.mkdirs()) {
                Log.w(TAG, "Failed to create share cache directory")
                return null
            }
            val target = File(targetDir, fileName)
            contentResolver.openInputStream(uri).use { input ->
                if (input == null) {
                    return null
                }
                if (!copyBounded(input, target)) {
                    Log.w(TAG, "Shared content exceeds the copy limit, dropping it")
                    targetDir.deleteRecursively()
                    return null
                }
            }
            target.absolutePath to contentResolver.getType(uri)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to copy shared content to cache", e)
            null
        }
    }

    // Returns false when the stream exceeds MAX_ATTACHMENT_COPY_BYTES.
    private fun copyBounded(input: InputStream, target: File): Boolean {
        target.outputStream().use { output ->
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            var total = 0L
            while (true) {
                val read = input.read(buffer)
                if (read < 0) {
                    return true
                }
                total += read
                if (total > MAX_ATTACHMENT_COPY_BYTES) {
                    return false
                }
                output.write(buffer, 0, read)
            }
        }
    }

    private fun queryDisplayName(uri: Uri): String? =
        contentResolver.query(
            uri,
            arrayOf(OpenableColumns.DISPLAY_NAME),
            null,
            null,
            null
        )?.use { cursor ->
            if (cursor.moveToFirst()) {
                val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (index >= 0) cursor.getString(index) else null
            } else {
                null
            }
        }

    private fun openMainApp() {
        val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
        if (launchIntent == null) {
            Log.w(TAG, "No launch intent for the main app")
            return
        }
        launchIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivity(launchIntent)
    }
}
