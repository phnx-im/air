// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.app.RemoteInput
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val LOGTAG = "NotificationReply"

// The "Reply" notification action target
//
// `onReceive` runs on the main thread, so it hands off to a one-shot
// WorkManager job rather than doing the work here.
class NotificationReplyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val chatId = intent.getStringExtra(Notifications.EXTRAS_CHAT_ID_KEY)
        if (chatId.isNullOrEmpty()) {
            Log.w(LOGTAG, "Reply intent missing chat ID")
            return
        }
        val text = RemoteInput.getResultsFromIntent(intent)
            ?.getCharSequence(Notifications.KEY_TEXT_REPLY)
            ?.toString()
        if (text.isNullOrBlank()) {
            Log.w(LOGTAG, "Reply intent missing text")
            return
        }

        val request = OneTimeWorkRequestBuilder<NotificationReplyWorker>()
            .setInputData(
                workDataOf(
                    NotificationReplyWorker.KEY_CHAT_ID to chatId,
                    NotificationReplyWorker.KEY_TEXT to text
                )
            )
            .build()

        WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
            "notification_reply_$chatId",
            ExistingWorkPolicy.REPLACE,
            request
        )
    }
}

class NotificationReplyWorker(appContext: Context, params: WorkerParameters) :
    CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result =
        withContext(Dispatchers.IO) {
            val chatId = inputData.getString(KEY_CHAT_ID)
            val text = inputData.getString(KEY_TEXT)
            if (chatId.isNullOrEmpty() || text.isNullOrEmpty()) {
                return@withContext Result.failure()
            }

            val logFilePath = applicationContext.cacheDir.resolve("background.log").absolutePath

            try {
                NativeLib().sendReply(
                    IncomingReplyContent(
                        path = applicationContext.filesDir.absolutePath,
                        logFilePath = logFilePath,
                        chatId = chatId,
                        text = text
                    )
                )
                // Tapping an action button doesn't auto-dismiss the notification the way a
                // swipe-dismiss does, so cancel it once the reply actually lands.
                Notifications.cancelNotifications(applicationContext, arrayListOf(chatId))
                Result.success()
            } catch (t: Throwable) {
                if (runAttemptCount >= MAX_ATTEMPTS) {
                    Log.e(LOGTAG, "Failed to send reply, giving up", t)
                    Result.failure()
                } else {
                    Log.w(LOGTAG, "Failed to send reply, retrying", t)
                    Result.retry()
                }
            }
        }

    companion object {
        const val KEY_CHAT_ID = "chat_id"
        const val KEY_TEXT = "text"
        private const val MAX_ATTEMPTS = 5
    }
}
