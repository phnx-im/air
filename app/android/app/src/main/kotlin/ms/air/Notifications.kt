// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

package ms.air

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Typeface
import android.os.Build
import android.os.Bundle
import android.text.Spannable
import android.text.SpannableString
import android.text.style.StyleSpan
import android.util.Base64
import android.util.Log
import androidx.annotation.RequiresPermission
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.Person
import androidx.core.content.LocusIdCompat
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import kotlinx.serialization.*
import kotlinx.serialization.json.*
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

private const val LOGTAG = "NativeLib"
private const val NOTIF_LOGTAG = "Notifications"

@Serializable
data class IncomingNotificationContent(
    val title: String,
    val body: String,
    val data: String,
    val path: String,
    val logFilePath: String,
)

@Serializable
data class NotificationContent(
    val identifier: String,
    val title: String,
    val body: String,
    val chatId: ChatId?,
    val conversation: ConversationNotification? = null
)

@Serializable
data class ChatId(
    val uuid: String
)

// Structured payload for Android `MessagingStyle` conversation notifications
@Serializable
data class ConversationNotification(
    val chatTitle: String,
    val isGroup: Boolean,
    val ownDisplayName: String,
    val participants: List<ConversationParticipant>,
    val messages: List<ConversationMessage>,
    val alert: Boolean,
    // Newest displayed entry (RFC 3339)
    //
    // Echoed back on dismissal to advance the notification watermark. Opaque
    // to the Kotlin side.
    val newestTimestamp: String,
    // Base64 (standard alphabet, padded) on the JNI JSON path
    val chatAvatar: String? = null
)

@Serializable
data class ConversationParticipant(
    val uuid: String,
    val displayName: String,
    // Base64 (standard alphabet, padded) on the JNI JSON path
    val avatar: String? = null
)

@Serializable
data class ConversationMessage(
    // Absent for system messages (e.g. group membership changes)
    val senderUuid: String? = null,
    val text: String,
    val isReaction: Boolean,
    val timestamp: Long
)

// A chat published to the OS as a direct share target
class ShareTarget(
    val chatId: String,
    val title: String,
    val avatar: ByteArray?
)

@Serializable
data class NotificationBatch(
    val badgeCount: Int,
    val removals: List<String>,
    val additions: List<NotificationContent>
)

@Serializable
data class IncomingDismissalContent(
    val path: String,
    val logFilePath: String,
    val chatId: String,
    // The dismissed notification's newest displayed entry (RFC 3339)
    val newestTimestamp: String
)

data class NotificationHandle(
    val notificationId: String,
    val chatId: String?
)

class NativeLib {
    companion object {
        // Load the shared library
        init {
            System.loadLibrary("airapplogic")
        }

        // Declare the native method
        @JvmStatic
        external fun process_new_messages(content: String): String

        // Declare the native method
        //
        // Returns an empty string on success; throws on failure.
        @JvmStatic
        external fun notification_dismissed(content: String): String
    }

    // Wrapper to process new messages. Handles JSON
    // serialization/deserialization and memory cleanup.
    fun processNewMessages(input: IncomingNotificationContent): NotificationBatch? {
        Log.d(LOGTAG, "handleDataMessage")
        // Serialize input data to JSON
        val jsonInput = Json.encodeToString(IncomingNotificationContent.serializer(), input)

        // Call the Rust function
        val rawOutput: String
        try {
            rawOutput = process_new_messages(jsonInput)
        } catch (e: Exception) {
            Log.e(LOGTAG, "Error calling native function: ${e.message}")
            return null
        }

        // Deserialize the output JSON back into NotificationBatch
        val result: NotificationBatch = try {
            Json.decodeFromString(NotificationBatch.serializer(), rawOutput)
        } catch (e: Exception) {
            Log.e(LOGTAG, "Error decoding response JSON: ${e.message}")
            return null
        }

        return result
    }

    // Throws when the native call fails (e.g. DB locked), so that the caller
    // can retry the persistence.
    fun notificationDismissed(input: IncomingDismissalContent) {
        val jsonInput = Json.encodeToString(IncomingDismissalContent.serializer(), input)
        notification_dismissed(jsonInput)
    }
}

class Notifications {
    companion object JniNotifications {
        const val CHANNEL_ID = "Chats"
        private const val NOTIFICATION_ID = 0
        private const val GROUP_KEY = "chats"
        private const val SUMMARY_TAG = "summary"

        const val SELECT_NOTIFICATION: String = "SELECT_NOTIFICATION"

        /// Key for storing the chat id in the Intent extras field
        const val EXTRAS_NOTIFICATION_ID_KEY: String = "ms.air/notification_id"
        const val EXTRAS_CHAT_ID_KEY: String = "ms.air/chat_id"
        const val EXTRAS_NEWEST_TIMESTAMP_KEY: String = "ms.air/newest_timestamp"

        // Category required for the conversation shortcut
        private const val SHORTCUT_CATEGORY_CONVERSATION = "android.shortcut.conversation"
        private const val SHORTCUT_CATEGORY_SHARE_TARGET = "ms.air.shortcut.SHARE_TARGET"

        // Adaptive icon canvas and its safe zone (dp). Launchers and the
        // share sheet mask the icon to the safe zone.
        private const val ADAPTIVE_ICON_SIZE = 108
        private const val ADAPTIVE_ICON_SAFE_ZONE = 72

        // Every conversation shortcut we publish doubles as a share target,
        // whether it came from a notification or from the share target list.
        private val SHORTCUT_CATEGORIES =
            setOf(SHORTCUT_CATEGORY_CONVERSATION, SHORTCUT_CATEGORY_SHARE_TARGET)

        // Shortcut operations decode bitmaps and do binder calls, so they run
        // off the main thread. A single thread keeps them in call order, which
        // the Dart side relies on: a clear must not be overtaken by an earlier
        // publish.
        private val shortcutExecutor: ExecutorService = Executors.newSingleThreadExecutor()

        fun runShortcutOp(block: () -> Unit) {
            shortcutExecutor.execute(block)
        }

        fun showNotification(context: Context, content: NotificationContent) {
            if (ActivityCompat.checkSelfPermission(
                    context, Manifest.permission.POST_NOTIFICATIONS
                ) != PackageManager.PERMISSION_GRANTED
            ) {
                return
            }

            // Create notification channel (needed for Android 8+)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val notificationManager =
                    context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
                val channel = NotificationChannel(
                    CHANNEL_ID, "Chats", NotificationManager.IMPORTANCE_HIGH
                )
                notificationManager.createNotificationChannel(channel)
            }

            val conversation = content.conversation
            val chatId = content.chatId
            if (conversation != null && chatId != null) {
                showConversationNotification(context, content, conversation, chatId)
            } else {
                showPlainNotification(context, content)
            }
        }


        @RequiresPermission(Manifest.permission.POST_NOTIFICATIONS)
        private fun showPlainNotification(context: Context, content: NotificationContent) {
            val pendingIntent = PendingIntent.getActivity(
                context,
                // Unique identifier per intent to ensure that multiple
                // notifications don't overwrite each other's pending intent
                content.identifier.hashCode(),
                buildContentIntent(context, content),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )

            val extras = Bundle().apply {
                putString(EXTRAS_CHAT_ID_KEY, content.chatId?.uuid)
            }

            val notification =
                NotificationCompat.Builder(context, CHANNEL_ID)
                    .setContentTitle(content.title)
                    .setContentText(content.body)
                    .setSmallIcon(R.drawable.ic_notification)
                    .setContentIntent(pendingIntent)
                    .setDefaults(Notification.DEFAULT_ALL)
                    .setPriority(NotificationManagerCompat.IMPORTANCE_HIGH)
                    .setCategory(NotificationCompat.CATEGORY_MESSAGE)
                    .addExtras(extras)
                    .setGroup(GROUP_KEY)
                    .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
                    .build()

            NotificationManagerCompat.from(context)
                .notify(content.identifier, NOTIFICATION_ID, notification)
            postGroupSummary(context)
        }

        @RequiresPermission(Manifest.permission.POST_NOTIFICATIONS)
        private fun showConversationNotification(
            context: Context,
            content: NotificationContent,
            conversation: ConversationNotification,
            chatId: ChatId
        ) {
            // A silent rebuild (e.g. a message edit) only updates a displayed notification in
            // place, where `setOnlyAlertOnce` suppresses the sound. It must not create a new
            // notification: posting anew on the high-importance channel always rings.
            if (!conversation.alert && !isNotificationDisplayed(context, content.identifier)) {
                return
            }

            val chatUuid = chatId.uuid

            val pendingIntent = PendingIntent.getActivity(
                context,
                chatUuid.hashCode(),
                buildContentIntent(context, content),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )

            val deleteIntent = PendingIntent.getBroadcast(
                context,
                chatUuid.hashCode(),
                Intent(context, NotificationDismissReceiver::class.java).apply {
                    putExtra(EXTRAS_CHAT_ID_KEY, chatUuid)
                    putExtra(EXTRAS_NEWEST_TIMESTAMP_KEY, conversation.newestTimestamp)
                },
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )

            val extras = Bundle().apply {
                putString(EXTRAS_CHAT_ID_KEY, chatUuid)
            }

            val chatPerson = Person.Builder()
                .setKey(chatUuid)
                .setName(conversation.chatTitle.ifBlank { chatUuid })
                .apply {
                    decodeAvatarIcon(conversation.chatAvatar)?.let { setIcon(it) }
                }
                .build()

            val notification =
                NotificationCompat.Builder(context, CHANNEL_ID)
                    .setContentTitle(content.title)
                    .setContentText(content.body)
                    .setSmallIcon(R.drawable.ic_notification)
                    .setContentIntent(pendingIntent)
                    .setDeleteIntent(deleteIntent)
                    .setDefaults(Notification.DEFAULT_ALL)
                    .setPriority(NotificationManagerCompat.IMPORTANCE_HIGH)
                    .setCategory(NotificationCompat.CATEGORY_MESSAGE)
                    .addPerson(chatPerson)
                    .addExtras(extras)
                    .setStyle(buildMessagingStyle(conversation))
                    .setShortcutId(chatUuid)
                    .setLocusId(LocusIdCompat(chatUuid))
                    .setOnlyAlertOnce(!conversation.alert)
                    .setGroup(GROUP_KEY)
                    .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
                    .build()

            pushConversationShortcut(context, content, chatUuid, conversation)

            NotificationManagerCompat.from(context)
                .notify(content.identifier, NOTIFICATION_ID, notification)
            postGroupSummary(context)
        }

        // Posts the group summary that bundles all chat notifications under the
        // app name. Idempotent, silent (children alert).
        @RequiresPermission(Manifest.permission.POST_NOTIFICATIONS)
        private fun postGroupSummary(context: Context) {
            val pendingIntent = PendingIntent.getActivity(
                context,
                SUMMARY_TAG.hashCode(),
                Intent(context, MainActivity::class.java),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
            val summary = NotificationCompat.Builder(context, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentIntent(pendingIntent)
                .setGroup(GROUP_KEY)
                .setGroupSummary(true)
                .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_CHILDREN)
                .setOnlyAlertOnce(true)
                .build()
            NotificationManagerCompat.from(context).notify(SUMMARY_TAG, NOTIFICATION_ID, summary)
        }

        private fun isNotificationDisplayed(context: Context, tag: String): Boolean =
            NotificationManagerCompat.from(context).activeNotifications
                .any { it.tag == tag && it.id == NOTIFICATION_ID }

        // Content intent shared by the notification tap target and the conversation shortcut
        //
        // Makes tap routing identical for both.
        private fun buildContentIntent(context: Context, content: NotificationContent): Intent =
            Intent(context, MainActivity::class.java).apply {
                action = SELECT_NOTIFICATION
                putExtra(EXTRAS_NOTIFICATION_ID_KEY, content.identifier)
                putExtra(EXTRAS_CHAT_ID_KEY, content.chatId?.uuid)
            }

        // Launcher-tap intent of a share-target shortcut.
        private fun buildChatIntent(context: Context, chatUuid: String): Intent =
            Intent(context, MainActivity::class.java).apply {
                action = SELECT_NOTIFICATION
                putExtra(EXTRAS_NOTIFICATION_ID_KEY, chatUuid)
                putExtra(EXTRAS_CHAT_ID_KEY, chatUuid)
            }

        // Publishes the chat as a long-lived conversation shortcut, which
        // makes it a direct target in the system share sheet. Also reports
        // the shortcut as used.
        fun publishShareShortcut(context: Context, target: ShareTarget) {
            try {
                val icon = shortcutAvatarIcon(target.avatar)
                    ?: IconCompat.createWithResource(context, R.mipmap.ic_launcher)
                val person = Person.Builder()
                    .setKey(target.chatId)
                    .setName(target.title)
                    .build()
                val shortcut = ShortcutInfoCompat.Builder(context, target.chatId)
                    .setLongLived(true)
                    .setShortLabel(target.title)
                    .setPerson(person)
                    .setCategories(SHORTCUT_CATEGORIES)
                    .setLocusId(LocusIdCompat(target.chatId))
                    .setIcon(icon)
                    .setIntent(buildChatIntent(context, target.chatId))
                    .build()
                ShortcutManagerCompat.pushDynamicShortcut(context, shortcut)
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to publish share shortcut", e)
            }
        }

        // Tells the system that the chat was just used, so that the share
        // sheet ranks its shortcut.
        fun reportShareShortcutUsed(context: Context, chatId: String) {
            try {
                ShortcutManagerCompat.reportShortcutUsed(context, chatId)
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to report share shortcut usage", e)
            }
        }

        // Withdraws the chats offered in the share sheet and the launcher.
        // Pinned shortcuts stay: the platform lets only the user remove them.
        fun clearShareShortcuts(context: Context) {
            try {
                val ids = ShortcutManagerCompat.getShortcuts(
                    context,
                    ShortcutManagerCompat.FLAG_MATCH_DYNAMIC or
                        ShortcutManagerCompat.FLAG_MATCH_CACHED
                )
                    .filter { SHORTCUT_CATEGORY_CONVERSATION in it.categories.orEmpty() }
                    .map { it.id }
                if (ids.isNotEmpty()) {
                    ShortcutManagerCompat.removeLongLivedShortcuts(context, ids)
                }
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to clear share shortcuts", e)
            }
        }

        // Ids of the currently published chat shortcuts (dynamic and cached).
        fun shareShortcutIds(context: Context): List<String> =
            try {
                ShortcutManagerCompat.getShortcuts(
                    context,
                    ShortcutManagerCompat.FLAG_MATCH_DYNAMIC or
                        ShortcutManagerCompat.FLAG_MATCH_CACHED
                )
                    .filter { SHORTCUT_CATEGORY_CONVERSATION in it.categories.orEmpty() }
                    .map { it.id }
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to list share shortcuts", e)
                emptyList()
            }

        // Withdraws the given share targets.
        fun removeShareShortcuts(context: Context, ids: List<String>) {
            if (ids.isEmpty()) {
                return
            }
            try {
                ShortcutManagerCompat.removeLongLivedShortcuts(context, ids)
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to remove share shortcuts", e)
            }
        }

        private fun buildMessagingStyle(conversation: ConversationNotification): NotificationCompat.MessagingStyle {
            val user = Person.Builder()
                .setName(conversation.ownDisplayName)
                .build()
            val style = NotificationCompat.MessagingStyle(user)

            if (conversation.isGroup) {
                style.setGroupConversation(true)
                style.setConversationTitle(conversation.chatTitle)
            }

            val participantsByUuid = conversation.participants.associateBy { it.uuid }
            val personCache = mutableMapOf<String, Person>()
            // Nameless sender for system messages; their text is self-contained.
            val systemPerson = Person.Builder().setName("").build()
            for (message in conversation.messages) {
                val senderUuid = message.senderUuid
                val sender = if (senderUuid != null) {
                    personCache.getOrPut(senderUuid) {
                        buildParticipantPerson(
                            senderUuid,
                            participantsByUuid[senderUuid]
                        )
                    }
                } else {
                    systemPerson
                }
                val text: CharSequence =
                    if (message.isReaction) italicizeExceptEmoji(message.text) else message.text
                style.addMessage(text, message.timestamp, sender)
            }

            return style
        }

        private fun buildParticipantPerson(
            uuid: String,
            participant: ConversationParticipant?
        ): Person {
            val builder = Person.Builder()
                .setKey(uuid)
                .setName(participant?.displayName ?: uuid)
            decodeAvatarIcon(participant?.avatar)?.let { builder.setIcon(it) }
            return builder.build()
        }

        // Decodes a base64 avatar into an icon.
        //
        // Notification icons stay plain bitmaps. The notification framework
        // masks a Person icon itself.
        private fun decodeAvatarIcon(avatarBase64: String?): IconCompat? {
            if (avatarBase64.isNullOrEmpty()) return null
            return try {
                val bytes = Base64.decode(avatarBase64, Base64.DEFAULT)
                val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return null
                IconCompat.createWithBitmap(bitmap)
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to decode avatar", e)
                null
            }
        }

        // Avatar icon for a long-lived shortcut.
        //
        // The platform rejects plain bitmap icons on long-lived shortcuts
        // so this is an adaptive one.
        //
        // The launcher masks an adaptive icon to its own shape, but only
        // within the safe zone, so the square avatar is inset into the
        // canvas instead of filling it.
        private fun shortcutAvatarIcon(bytes: ByteArray?): IconCompat? {
            if (bytes == null || bytes.isEmpty()) return null
            return try {
                val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                    ?: return null
                IconCompat.createWithAdaptiveBitmap(insetToSafeZone(cropToSquare(bitmap)))
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to decode shortcut avatar", e)
                null
            }
        }

        // Variant for the JNI notification path, which carries avatars as
        // base64.
        private fun shortcutAvatarIconFromBase64(avatarBase64: String?): IconCompat? {
            if (avatarBase64.isNullOrEmpty()) return null
            return try {
                shortcutAvatarIcon(Base64.decode(avatarBase64, Base64.DEFAULT))
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to decode shortcut avatar", e)
                null
            }
        }

        // Center-crops the bitmap to a square.
        private fun cropToSquare(source: Bitmap): Bitmap {
            val side = minOf(source.width, source.height)
            if (source.width == side && source.height == side) {
                return source
            }
            return Bitmap.createBitmap(
                source,
                (source.width - side) / 2,
                (source.height - side) / 2,
                side,
                side
            )
        }

        // Centers the square bitmap in the safe zone of a transparent
        // adaptive icon canvas.
        private fun insetToSafeZone(square: Bitmap): Bitmap {
            val canvasSide = square.width * ADAPTIVE_ICON_SIZE / ADAPTIVE_ICON_SAFE_ZONE
            val offset = ((canvasSide - square.width) / 2).toFloat()
            val canvas = Bitmap.createBitmap(canvasSide, canvasSide, Bitmap.Config.ARGB_8888)
            Canvas(canvas).drawBitmap(square, offset, offset, null)
            return canvas
        }

        // Italicizes a reaction line, except emoji code points.
        private fun italicizeExceptEmoji(text: String): CharSequence {
            val spannable = SpannableString(text)
            var index = 0
            var runStart = -1
            while (index < text.length) {
                val codePoint = text.codePointAt(index)
                val charCount = Character.charCount(codePoint)
                if (isEmojiCodePoint(codePoint)) {
                    if (runStart >= 0) {
                        spannable.setSpan(
                            StyleSpan(Typeface.ITALIC),
                            runStart, index,
                            Spannable.SPAN_EXCLUSIVE_EXCLUSIVE
                        )
                        runStart = -1
                    }
                } else if (runStart < 0) {
                    runStart = index
                }
                index += charCount
            }
            if (runStart >= 0) {
                spannable.setSpan(
                    StyleSpan(Typeface.ITALIC),
                    runStart, text.length,
                    Spannable.SPAN_EXCLUSIVE_EXCLUSIVE
                )
            }
            return spannable
        }

        // Simple emoji code point check.
        //
        // TODO: Is it good enough?
        private fun isEmojiCodePoint(codePoint: Int): Boolean =
            Character.isSupplementaryCodePoint(codePoint) ||
                    codePoint in 0x2600..0x27BF ||
                    codePoint in 0xFE00..0xFE0F ||
                    codePoint == 0x200D

        // Publishes/refreshes the conversation's longed-lived launcher
        // shortcut.
        //
        // Required for the notification to appear in the OS Conversations
        // section. A failure must not block the notification.
        private fun pushConversationShortcut(
            context: Context,
            content: NotificationContent,
            chatUuid: String,
            conversation: ConversationNotification
        ) {
            try {
                val newestMessage = conversation.messages.lastOrNull()
                val senderParticipant =
                    conversation.participants.find { it.uuid == newestMessage?.senderUuid }
                val shortLabel = listOf(
                    conversation.chatTitle,
                    senderParticipant?.displayName ?: "",
                    chatUuid
                ).first { it.isNotBlank() }

                val person = Person.Builder()
                    .setKey(senderParticipant?.uuid ?: chatUuid)
                    .setName(senderParticipant?.displayName ?: shortLabel)
                    .apply {
                        decodeAvatarIcon(senderParticipant?.avatar)?.let { setIcon(it) }
                    }
                    .build()

                val icon = shortcutAvatarIconFromBase64(conversation.chatAvatar)
                    ?: IconCompat.createWithResource(context, R.mipmap.ic_launcher)

                // TODO: Do we have to set isGroup here?
                val shortcut = ShortcutInfoCompat.Builder(context, chatUuid)
                    .setLongLived(true)
                    .setShortLabel(shortLabel)
                    .setPerson(person)
                    .setCategories(SHORTCUT_CATEGORIES)
                    .setLocusId(LocusIdCompat(chatUuid))
                    .setIcon(icon)
                    .setIntent(buildContentIntent(context, content))
                    .build()

                ShortcutManagerCompat.pushDynamicShortcut(context, shortcut)
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to push conversation shortcut", e)
            }
        }

        fun getActiveNotifications(context: Context): Array<NotificationHandle> {
            return NotificationManagerCompat.from(context).activeNotifications
                .mapNotNull { sbn ->
                    if (sbn.tag == SUMMARY_TAG) return@mapNotNull null
                    NotificationHandle(
                        sbn.tag,
                        sbn.notification.extras.getString(EXTRAS_CHAT_ID_KEY)
                    )
                }
                .toTypedArray()
        }

        fun cancelNotifications(context: Context, identifiers: ArrayList<String>) {
            val notificationManager =
                context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            for (identifier in identifiers) {
                notificationManager.cancel(identifier, NOTIFICATION_ID)
            }

            // The summary is not auto-removed on programmatic cancel and would
            // linger as an empty notification. `cancel` is asynchronous, so the
            // just-cancelled identifiers may still be listed as active.
            val hasChildren = NotificationManagerCompat.from(context).activeNotifications
                .any { it.id == NOTIFICATION_ID && it.tag != SUMMARY_TAG && it.tag !in identifiers }
            if (!hasChildren) {
                notificationManager.cancel(SUMMARY_TAG, NOTIFICATION_ID)
            }
        }
    }
}
