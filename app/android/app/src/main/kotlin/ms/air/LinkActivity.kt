// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity

// Receives contact links and hands them to the main app. No UI.
class LinkActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val link = intent.dataString
        if (intent.action == Intent.ACTION_VIEW && link != null) {
            val forward = Intent(this, MainActivity::class.java).apply {
                action = MainActivity.OPEN_LINK
                putExtra(MainActivity.EXTRA_LINK, link)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(forward)
        }
        finish()
    }
}
