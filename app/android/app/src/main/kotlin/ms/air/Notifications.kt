// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import kotlinx.serialization.*
import kotlinx.serialization.json.*

private const val LOGTAG = "NativeLib"

@Serializable
data class IncomingNotificationContent(
    val title: String,
    val body: String,
    val data: String,
    val path: String,
    val logFilePath: String,
)

@Serializable
data class NotificationContent(
    val identifier: String,
    val title: String,
    val body: String,
    val chatId: ChatId?
)

@Serializable
data class ChatId(
    val uuid: String
)

@Serializable
data class NotificationBatch(
    val badgeCount: Int,
    val removals: List<String>,
    val additions: List<NotificationContent>
)

data class NotificationHandle(
    val notificationId: String,
    val chatId: String?
)

class NativeLib {
    companion object {
        // Load the shared library
        init {
            System.loadLibrary("airapplogic")
        }

        // Declare the native method
        @JvmStatic
        external fun process_new_messages(content: String): String
    }

    // Wrapper to process new messages. Handles JSON
    // serialization/deserialization and memory cleanup.
    fun processNewMessages(input: IncomingNotificationContent): NotificationBatch? {
        Log.d(LOGTAG, "handleDataMessage")
        // Serialize input data to JSON
        val jsonInput = Json.encodeToString(IncomingNotificationContent.serializer(), input)

        // Call the Rust function
        val rawOutput: String
        try {
            rawOutput = process_new_messages(jsonInput)
        } catch (e: Exception) {
            Log.e(LOGTAG, "Error calling native function: ${e.message}")
            return null
        }

        // Deserialize the output JSON back into NotificationBatch
        val result: NotificationBatch = try {
            Json.decodeFromString(NotificationBatch.serializer(), rawOutput)
        } catch (e: Exception) {
            Log.e(LOGTAG, "Error decoding response JSON: ${e.message}")
            return null
        }

        return result
    }
}

class Notifications {
    companion object JniNotifications {
        private const val CHANNEL_ID = "Chats"
        private const val NOTIFICATION_ID = 0

        const val SELECT_NOTIFICATION: String = "SELECT_NOTIFICATION"

        /// Key for storing the chat id in the Intent extras field
        const val EXTRAS_NOTIFICATION_ID_KEY: String = "ms.air/notification_id"
        const val EXTRAS_CHAT_ID_KEY: String = "ms.air/chat_id"


        fun showNotification(context: Context, content: NotificationContent) {
            if (ActivityCompat.checkSelfPermission(
                    context, Manifest.permission.POST_NOTIFICATIONS
                ) != PackageManager.PERMISSION_GRANTED
            ) {
                return
            }

            val notificationManager =
                context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

            // Create notification channel (needed for Android 8+)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val channel = NotificationChannel(
                    CHANNEL_ID, "Chats", NotificationManager.IMPORTANCE_HIGH
                )
                notificationManager.createNotificationChannel(channel)
            }

            val intent = Intent(context, MainActivity::class.java).apply {
                action = SELECT_NOTIFICATION
                putExtra(EXTRAS_NOTIFICATION_ID_KEY, content.identifier)
                putExtra(EXTRAS_CHAT_ID_KEY, content.chatId?.uuid)
            }

            val pendingIntent = PendingIntent.getActivity(
                context,
                1,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )

            val extras = Bundle().apply {
                putString(EXTRAS_CHAT_ID_KEY, content.chatId?.uuid)
            }

            val notification =
                NotificationCompat.Builder(context, CHANNEL_ID)
                    .setContentTitle(content.title)
                    .setContentText(content.body)
                    .setSmallIcon(android.R.drawable.ic_notification_overlay)
                    .setContentIntent(pendingIntent)
                    .setDefaults(Notification.DEFAULT_ALL)
                    .setPriority(NotificationManagerCompat.IMPORTANCE_HIGH)
                    .addExtras(extras)
                    .build()

            NotificationManagerCompat.from(context)
                .notify(content.identifier, NOTIFICATION_ID, notification)
        }

        fun getActiveNotifications(context: Context): Array<NotificationHandle> {
            return NotificationManagerCompat.from(context).activeNotifications
                .mapNotNull { sbn ->
                    NotificationHandle(
                        sbn.tag,
                        sbn.notification.extras.getString(EXTRAS_CHAT_ID_KEY)
                    )
                }
                .toTypedArray()
        }

        fun cancelNotifications(context: Context, identifiers: ArrayList<String>) {
            val notificationManager =
                context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            for (identifier in identifiers) {
                notificationManager.cancel(identifier, NOTIFICATION_ID)
            }
        }
    }
}
