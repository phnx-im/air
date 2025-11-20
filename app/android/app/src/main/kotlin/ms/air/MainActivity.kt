// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.Manifest
import android.content.ContentValues.TAG
import android.content.Intent
import android.content.pm.PackageManager
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

class MainActivity : FlutterActivity() {
    companion object {
        private const val CHANNEL_NAME: String = "ms.air/channel"
        private const val REQUEST_CODE_POST_NOTIFICATIONS = 1000
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

                "getDownloadsDirectory" -> {
                    val path = Environment
                        .getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
                        .absolutePath
                    result.success(path)
                }

                "getPicturesDirectory" -> {
                    val path = Environment
                        .getExternalStoragePublicDirectory(Environment.DIRECTORY_PICTURES)
                        .absolutePath
                    result.success(path)
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
}
