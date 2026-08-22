// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.core.content.IntentCompat
import io.flutter.Log
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.InputStream
import java.util.UUID
import java.util.concurrent.CopyOnWriteArrayList

// Hosts the Flutter share UI in its own engine running the `shareMain`
// entrypoint.
class ShareActivity : FlutterActivity() {
    companion object {
        private const val TAG = "ShareActivity"
        private const val APP_CHANNEL_NAME = "ms.air/channel"
        private const val SHARE_CHANNEL_NAME = "ms.air/share"
        private const val SHARE_CACHE_DIR = "share"

        // Matches ShareCubitBase::MAX_SHARED_ATTACHMENTS. One extra URI is
        // copied so the Rust side still sees too many items and reports the
        // too-many-attachments error.
        private const val MAX_ATTACHMENTS = 10

        // Upper bound for copying a single shared stream. Tracks the deployed
        // `max_attachment_size` (see StorageSettings in the backend) with
        // slack.
        private const val MAX_ATTACHMENT_COPY_BYTES = 32L * 1024 * 1024

        // Cache entries older than this are considered leftovers of a
        // killed share session and are removed on the next share.
        private const val STALE_CACHE_AGE_MILLIS = 24L * 60 * 60 * 1000
    }

    private val scope = MainScope()

    private val createdCacheDirs = CopyOnWriteArrayList<File>()

    override fun getDartEntrypointFunctionName(): String = "shareMain"

    override fun onDestroy() {
        scope.cancel()
        val dirs = createdCacheDirs.toList()
        createdCacheDirs.clear()
        // The scope is cancelled, so clean up on a plain background thread.
        Thread {
            dirs.forEach { it.deleteRecursively() }
            deleteStaleCacheDirs()
        }.start()
        super.onDestroy()
    }

    // Removes cache directories left behind by share sessions that were
    // killed before their onDestroy cleanup ran.
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

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

        // Minimal subset of the main app channel used by the share engine
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, APP_CHANNEL_NAME)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "getDatabasesDirectory" -> result.success(filesDir.absolutePath)
                    else -> result.notImplemented()
                }
            }

        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, SHARE_CHANNEL_NAME)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "getSharedPayload" -> getSharedPayload(result)

                    "close" -> {
                        result.success(null)
                        finish()
                    }

                    "openMainApp" -> {
                        openMainApp()
                        result.success(null)
                        finish()
                    }

                    else -> result.notImplemented()
                }
            }
    }

    // Content URIs are copied into the app cache off the main thread,
    // because the read permission on them is transient.
    private fun getSharedPayload(result: MethodChannel.Result) {
        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        val shareTargetIdentifier = intent.getStringExtra(Intent.EXTRA_SHORTCUT_ID)
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

        scope.launch {
            val candidates = uris.take(MAX_ATTACHMENTS + 1)
            val attachments = withContext(Dispatchers.IO) {
                candidates.mapNotNull { uri -> copyToCache(uri) }
            }
            result.success(
                mapOf(
                    "text" to text,
                    "attachments" to attachments,
                    // Unreadable and oversized streams are skipped above. The
                    // share UI says so instead of reporting a silent success.
                    "droppedAttachments" to candidates.size - attachments.size,
                    "shareTargetIdentifier" to shareTargetIdentifier,
                )
            )
        }
    }

    // Returns the `{path, mimeType}` entry for the payload, or null when the
    // URI cannot be read.
    private fun copyToCache(uri: Uri): Map<String, String?>? {
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
            createdCacheDirs.add(targetDir)
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
            mapOf(
                "path" to target.absolutePath,
                "mimeType" to contentResolver.getType(uri),
            )
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
