// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.Manifest
import android.content.ContentValues
import android.content.ContentValues.TAG
import android.content.Intent
import android.content.pm.PackageManager
import android.media.MediaScannerConnection
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.google.android.gms.tasks.Task
import com.google.firebase.messaging.FirebaseMessaging
import io.flutter.Log
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import android.os.Environment
import android.provider.MediaStore
import androidx.core.net.toUri
import java.io.File
import java.io.IOException

class MainActivity : FlutterActivity() {
    companion object {
        private const val CHANNEL_NAME: String = "ms.air/channel"
        private const val REQUEST_CODE_POST_NOTIFICATIONS = 1000
        private const val APP_DIR_NAME: String = "Air"
    }

    private var channel: MethodChannel? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestNotificationPermissionIfNeeded()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)

        if (intent.action == Notifications.SELECT_NOTIFICATION) {
            val notificationId = intent.extras?.getString(Notifications.EXTRAS_NOTIFICATION_ID_KEY)
            val chatId = intent.extras?.getString(Notifications.EXTRAS_CHAT_ID_KEY)
            if (notificationId != null) {
                val arguments = mapOf(
                    "identifier" to notificationId, "chatId" to chatId
                )
                channel?.invokeMethod("openedNotification", arguments)
            }
        }
    }

    override fun detachFromFlutterEngine() {
        super.detachFromFlutterEngine()

        channel?.setMethodCallHandler(null)
        channel = null
    }

    // Configures the Method Channel to communicate with Flutter
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

        channel = MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger, CHANNEL_NAME
        )
        channel?.setMethodCallHandler { call, result ->
            when (call.method) {
                "getDeviceToken" -> {
                    FirebaseMessaging.getInstance().token.addOnCompleteListener { task: Task<String> ->
                        if (task.isSuccessful) {
                            val token = task.result
                            result.success(token)
                        } else {
                            Log.w(TAG, "Fetching FCM registration token failed" + task.exception)
                            result.error("NoDeviceToken", "Device token not available", "")
                        }
                    }
                }

                "getDatabasesDirectory" -> {
                    val databasePath = filesDir.absolutePath
                    Log.d(TAG, "Application database path: $databasePath")
                    result.success(databasePath)
                }

                "sendNotification" -> {
                    val identifier: String? = call.argument("identifier")
                    val title: String? = call.argument("title")
                    val body: String? = call.argument("body")
                    val chatId: ChatId? =
                        call.argument<String>("chatId")?.let { ChatId(it) }

                    if (identifier != null && title != null && body != null) {
                        val notification =
                            NotificationContent(identifier, title, body, chatId)
                        Notifications.showNotification(this, notification)
                        result.success(null)
                    } else {
                        result.error(
                            "DeserializeError",
                            "Failed to decode notification arguments ${call.arguments}",
                            ""
                        )
                    }
                }

                "getActiveNotifications" -> {
                    val notifications = Notifications.getActiveNotifications(this)
                    val res: ArrayList<Map<String, Any?>> = ArrayList(notifications.map { handle ->
                        mapOf<String, Any?>(
                            "identifier" to handle.notificationId,
                            "chatId" to handle.chatId
                        )
                    })
                    result.success(res)
                }

                "cancelNotifications" -> {
                    val identifiers: ArrayList<String>? = call.argument("identifiers")
                    if (identifiers != null) {
                        Notifications.cancelNotifications(this, identifiers)
                    } else {
                        result.error(
                            "DeserializeError", "Failed to decode 'identifiers' arguments", ""
                        )
                    }
                }

                "saveFile" -> {
                    val fileName = call.argument<String>("fileName")
                    val mimeType = call.argument<String>("mimeType")
                    val data = call.argument<ByteArray>("data")

                    if (fileName == null || mimeType == null || data == null) {
                        result.error(
                            "INVALID_ARGUMENTS",
                            "File name, MIME type, or data not provided",
                            null
                        )
                    } else {
                        saveFile(fileName, mimeType, data, result)
                    }
                }

                else -> {
                    result.notImplemented()
                }
            }
        }
    }

    private fun requestNotificationPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            return
        }

        val hasPermission = ContextCompat.checkSelfPermission(
            this,
            Manifest.permission.POST_NOTIFICATIONS
        ) == PackageManager.PERMISSION_GRANTED

        if (hasPermission) {
            return
        }

        ActivityCompat.requestPermissions(
            this,
            arrayOf(Manifest.permission.POST_NOTIFICATIONS),
            REQUEST_CODE_POST_NOTIFICATIONS
        )
    }

    private fun saveFile(
        fileName: String,
        mimeType: String,
        data: ByteArray,
        result: MethodChannel.Result
    ) {
        var finalUri: Uri? = null
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                val collectionUri = when {
                    mimeType.startsWith("image/") -> MediaStore.Images.Media.getContentUri(
                        MediaStore.VOLUME_EXTERNAL_PRIMARY
                    )

                    mimeType.startsWith("video/") -> MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
                    mimeType.startsWith("audio/") -> MediaStore.Audio.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
                    else -> MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
                }

                val contentValues = ContentValues().apply {
                    put(MediaStore.MediaColumns.DISPLAY_NAME, fileName)
                    put(MediaStore.MediaColumns.MIME_TYPE, mimeType)
                    val relativePath = when {
                        mimeType.startsWith("image/") -> Environment.DIRECTORY_PICTURES
                        mimeType.startsWith("video/") -> Environment.DIRECTORY_MOVIES
                        mimeType.startsWith("audio/") -> Environment.DIRECTORY_MUSIC
                        else -> Environment.DIRECTORY_DOWNLOADS
                    }
                    put(MediaStore.MediaColumns.RELATIVE_PATH, relativePath + "/" + APP_DIR_NAME)
                    put(MediaStore.MediaColumns.IS_PENDING, 1)
                }

                val resolver = context.contentResolver
                val uri = resolver.insert(collectionUri, contentValues)
                    ?: throw IOException("Failed to create new MediaStore record.")

                finalUri = uri

                // Write the data
                resolver.openOutputStream(uri).use { outputStream ->
                    outputStream?.write(data) ?: throw IOException("Failed to get output stream.")
                }

                // Finalize the file
                contentValues.clear()
                contentValues.put(MediaStore.MediaColumns.IS_PENDING, 0)
                resolver.update(uri, contentValues, null, null)

                Log.d(TAG, "Successfully saved file to content:// URI: $uri")
                result.success(null)
            } else {
                // --- LEGACY PATH for Android 9 (API 28) and older ---
                val directoryType = when {
                    mimeType.startsWith("image/") -> Environment.DIRECTORY_PICTURES
                    mimeType.startsWith("video/") -> Environment.DIRECTORY_MOVIES
                    mimeType.startsWith("audio/") -> Environment.DIRECTORY_MUSIC
                    else -> Environment.DIRECTORY_DOWNLOADS
                }

                @Suppress("DEPRECATION")
                val directory = Environment.getExternalStoragePublicDirectory(directoryType)
                val appDirectory = File(directory, APP_DIR_NAME)
                if (!appDirectory.exists() && !appDirectory.mkdirs()) {
                    throw IOException("Failed to create directory")
                }

                val file = File(appDirectory, fileName)
                finalUri = Uri.fromFile(file)

                // Write the data
                file.outputStream().use { it.write(data) }

                // Finalize (scan the file)
                MediaScannerConnection.scanFile(
                    context,
                    arrayOf(file.absolutePath),
                    arrayOf(mimeType),
                    null
                )

                Log.d(TAG, "Successfully saved file to file:// URI: $finalUri")
                result.success(null)
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error saving file", e)
            // If we're on modern Android and an error occurred after creating the URI, delete the orphan entry
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && finalUri != null) {
                context.contentResolver.delete(finalUri, null, null)
            }
            result.error("SAVE_ERROR", "Failed to save file: ${e.message}", null)
        }
    }
}
