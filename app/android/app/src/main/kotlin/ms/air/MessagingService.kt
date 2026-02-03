// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.util.Log
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.workDataOf
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage

private const val LOGTAG = "MessagingService"

class BackgroundFirebaseMessagingService : FirebaseMessagingService() {
    // Handle incoming messages from the OS
    override fun onMessageReceived(remoteMessage: RemoteMessage) {
        Log.d(LOGTAG, "onMessageReceived")
        val isHighPriority =
            remoteMessage.priority == RemoteMessage.PRIORITY_HIGH ||
                    remoteMessage.originalPriority == RemoteMessage.PRIORITY_HIGH
        enqueueDataMessage(remoteMessage.data, isHighPriority)
    }

    private fun enqueueDataMessage(data: Map<String, String>, isHighPriority: Boolean) {
        Log.d(LOGTAG, "enqueueDataMessage highPriority=$isHighPriority")
        val workData =
            workDataOf(
                PushProcessingWorker.KEY_DATA_PAYLOAD to (data["data"] ?: ""),
            )
        val requestBuilder =
            OneTimeWorkRequestBuilder<PushProcessingWorker>().setInputData(workData)
        if (isHighPriority) {
            requestBuilder.setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
        }
        WorkManager.getInstance(applicationContext).enqueue(requestBuilder.build())
    }

    override fun onNewToken(token: String) {
        // Handle token refresh
        Log.w(LOGTAG, "Device token was updated")
        // TODO: The new token needs to be provisioned on the server
    }
}
