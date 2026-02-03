// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.content.Context
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val WORKER_LOGTAG = "PushProcessingWorker"

class PushProcessingWorker(
    appContext: Context,
    params: WorkerParameters,
) : CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result =
        withContext(Dispatchers.IO) {
            val dataPayload = inputData.getString(KEY_DATA_PAYLOAD) ?: ""
            val logFilePath = applicationContext.cacheDir.resolve("background.log").absolutePath
            Log.d(WORKER_LOGTAG, "Logging file path: $logFilePath")

            val notificationContent =
                IncomingNotificationContent(
                    title = "",
                    body = "",
                    data = dataPayload,
                    path = applicationContext.filesDir.absolutePath,
                    logFilePath = logFilePath,
                )

            try {
                Log.d(WORKER_LOGTAG, "Starting to process messages in Rust")
                val notificationBatch = NativeLib().processNewMessages(notificationContent)
                Log.d(WORKER_LOGTAG, "Finished to process messages in Rust")

                // Show the notifications
                notificationBatch?.additions?.forEach { content ->
                    Notifications.showNotification(applicationContext, content)
                }

                // Remove the notifications
                if (notificationBatch?.removals != null) {
                    Notifications.cancelNotifications(
                        applicationContext,
                        ArrayList(notificationBatch.removals)
                    )
                }

                Result.success()
            } catch (t: Throwable) {
                Log.e(WORKER_LOGTAG, "Failed to process messages", t)
                Result.retry()
            }
        }

    companion object {
        const val KEY_DATA_PAYLOAD = "data_payload"
    }
}
