// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.os.Handler
import android.os.Looper

/**
 * Challenges that arrived over FCM for the sign-up flow.
 *
 * A challenge is kept as well as delivered, since it can arrive before Dart is
 * listening. The latest one wins.
 */
object AdmissionChallenges {
    @Volatile
    private var pending: String? = null

    fun publish(challenge: String) {
        pending = challenge
        val channel = MainActivity.activeChannel() ?: return
        Handler(Looper.getMainLooper()).post {
            channel.invokeMethod("receivedAdmissionChallenge", challenge)
        }
    }

    fun take(): String? {
        val challenge = pending
        pending = null
        return challenge
    }
}
