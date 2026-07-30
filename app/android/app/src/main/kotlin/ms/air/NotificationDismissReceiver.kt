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

private const val LOGTAG = "NotificationDismiss"

// The notification's `deleteIntent` target
//
// `onReceive` runs on the main thread, so it hands off to a one-short
// WorkManager job rather than doing the work here.
class NotificationDismissReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val chatId = intent.getStringExtra(Notifications.EXTRAS_CHAT_ID_KEY)
        if (chatId.isNullOrEmpty()) {
            Log.w(LOGTAG, "Dismiss intent missing chat ID")
            return
        }
        val newestTimestamp = intent.getStringExtra(Notifications.EXTRAS_NEWEST_TIMESTAMP_KEY)
        if (newestTimestamp.isNullOrEmpty()) {
            Log.w(LOGTAG, "Dismiss intent missing newest timestamp")
            return
        }

        val request = OneTimeWorkRequestBuilder<NotificationDismissWorker>()
            .setInputData(
                workDataOf(
                    NotificationDismissWorker.KEY_CHAT_ID to chatId,
                    NotificationDismissWorker.KEY_NEWEST_TIMESTAMP to newestTimestamp
                )
            )
            .build()

        WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
            "notification_dismiss_$chatId",
            ExistingWorkPolicy.REPLACE,
            request
        )
    }
}

class NotificationDismissWorker(appContext: Context, params: WorkerParameters) :
    CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result =
        withContext(Dispatchers.IO) {
            val chatId = inputData.getString(KEY_CHAT_ID)
            val newestTimestamp = inputData.getString(KEY_NEWEST_TIMESTAMP)
            if (chatId.isNullOrEmpty() || newestTimestamp.isNullOrEmpty()) {
                return@withContext Result.failure()
            }

            val logFilePath = applicationContext.cacheDir.resolve("background.log").absolutePath

            try {
                NativeLib().notificationDismissed(
                    IncomingDismissalContent(
                        path = applicationContext.filesDir.absolutePath,
                        logFilePath = logFilePath,
                        chatId = chatId,
                        newestTimestamp = newestTimestamp
                    )
                )
                Result.success()
            } catch (t: Throwable) {
                // Retry transient failures (e.g. DB locked); losing the watermark update
                // re-shows already dismissed notifications.
                if (runAttemptCount >= MAX_ATTEMPTS) {
                    Log.e(LOGTAG, "Failed to persist notification dismissal, giving up", t)
                    Result.failure()
                } else {
                    Log.w(LOGTAG, "Failed to persist notification dismissal, retrying", t)
                    Result.retry()
                }
            }
        }

    companion object {
        const val KEY_CHAT_ID = "chat_id"
        const val KEY_NEWEST_TIMESTAMP = "newest_timestamp"
        private const val MAX_ATTEMPTS = 5
    }
}
