// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val LOGTAG = "NotificationMarkAsRead"

// The "Mark as read" notification action target
//
// `onReceive` runs on the main thread, so it hands off to a one-shot
// WorkManager job rather than doing the work here.
class NotificationMarkAsReadReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val chatId = intent.getStringExtra(Notifications.EXTRAS_CHAT_ID_KEY)
        if (chatId.isNullOrEmpty()) {
            Log.w(LOGTAG, "Mark-as-read intent missing chat ID")
            return
        }
        val messageId = intent.getStringExtra(Notifications.EXTRAS_MESSAGE_ID_KEY)
        if (messageId.isNullOrEmpty()) {
            Log.w(LOGTAG, "Mark-as-read intent missing message ID")
            return
        }

        val request = OneTimeWorkRequestBuilder<NotificationMarkAsReadWorker>()
            .setInputData(
                workDataOf(
                    NotificationMarkAsReadWorker.KEY_CHAT_ID to chatId,
                    NotificationMarkAsReadWorker.KEY_MESSAGE_ID to messageId
                )
            )
            .build()

        WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
            "notification_mark_as_read_$chatId",
            ExistingWorkPolicy.REPLACE,
            request
        )
    }
}

class NotificationMarkAsReadWorker(appContext: Context, params: WorkerParameters) :
    CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result =
        withContext(Dispatchers.IO) {
            val chatId = inputData.getString(KEY_CHAT_ID)
            val messageId = inputData.getString(KEY_MESSAGE_ID)
            if (chatId.isNullOrEmpty() || messageId.isNullOrEmpty()) {
                return@withContext Result.failure()
            }

            val logFilePath = applicationContext.cacheDir.resolve("background.log").absolutePath

            try {
                NativeLib().markAsRead(
                    IncomingMarkAsReadContent(
                        path = applicationContext.filesDir.absolutePath,
                        logFilePath = logFilePath,
                        chatId = chatId,
                        messageId = messageId
                    )
                )
                // Tapping an action button doesn't auto-dismiss the notification the way a
                // swipe-dismiss does, so cancel it once the mark-as-read actually lands.
                Notifications.cancelNotifications(applicationContext, arrayListOf(chatId))
                Result.success()
            } catch (t: Throwable) {
                if (runAttemptCount >= MAX_ATTEMPTS) {
                    Log.e(LOGTAG, "Failed to mark chat as read, giving up", t)
                    Result.failure()
                } else {
                    Log.w(LOGTAG, "Failed to mark chat as read, retrying", t)
                    Result.retry()
                }
            }
        }

    companion object {
        const val KEY_CHAT_ID = "chat_id"
        const val KEY_MESSAGE_ID = "message_id"
        private const val MAX_ATTEMPTS = 5
    }
}
