// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.os.Handler
import android.os.Looper
import java.util.concurrent.atomic.AtomicReference

/**
 * Challenges that arrived over FCM for the sign-up flow.
 *
 * A challenge is kept as well as delivered, since it can arrive before Dart is
 * listening. The latest one wins.
 */
object AdmissionChallenges {
    private val pending = AtomicReference<Map<String, String>?>(null)

    fun publish(sessionId: String, challenge: String) {
        val arguments = mapOf("sessionId" to sessionId, "challenge" to challenge)
        pending.set(arguments)
        val channel = MainActivity.activeChannel() ?: return
        Handler(Looper.getMainLooper()).post {
            channel.invokeMethod("receivedAdmissionChallenge", arguments)
        }
    }

    fun take(): Map<String, String>? = pending.getAndSet(null)
}
