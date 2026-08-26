// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import java.util.concurrent.atomic.AtomicReference

// Handoff slot from ShareActivity to MainActivity in shared memory.
//
// MainActivity is exported to the launcher, so any app can start it with
// arbitrary intents. A payload kept in process memory cannot be forget that
// way.
//
// Relies on both activities sharing one process, so neither may get an
// `android:process` attribute.
object PendingShare {
    private val payload = AtomicReference<Map<String, Any?>?>(null)

    fun put(payload: Map<String, Any?>) {
        this.payload.set(payload)
    }

    // Returns the pending payload and clears the slot.
    fun take(): Map<String, Any?>? = payload.getAndSet(null)
}
