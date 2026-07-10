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

        val request = OneTimeWorkRequestBuilder<NotificationDismissWorker>()
            .setInputData(workDataOf(NotificationDismissWorker.KEY_CHAT_ID to chatId))
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
            if (chatId.isNullOrEmpty()) {
                return@withContext Result.failure()
            }

            val logFilePath = applicationContext.cacheDir.resolve("background.log").absolutePath

            try {
                NativeLib().notificationDismissed(
                    IncomingDismissalContent(
                        path = applicationContext.filesDir.absolutePath,
                        logFilePath = logFilePath,
                        chatId = chatId
                    )
                )
                Result.success()
            } catch (t: Throwable) {
                Log.e(LOGTAG, "Failed to persist notification dismissal", t)
                Result.failure()
            }
        }

    companion object {
        const val KEY_CHAT_ID = "chat_id"
    }
}
