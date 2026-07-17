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
import android.graphics.Paint
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import android.graphics.Rect
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
import androidx.core.graphics.createBitmap

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
    val alert: Boolean
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
    val senderUuid: String,
    val text: String,
    val isReaction: Boolean,
    val timestamp: Long
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
    val chatId: String
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

    fun notificationDismissed(input: IncomingDismissalContent) {
        val jsonInput = Json.encodeToString(IncomingDismissalContent.serializer(), input)
        try {
            notification_dismissed(jsonInput)
        } catch (e: Exception) {
            Log.e(LOGTAG, "Error calling native notification_dismissed function: ${e.message}")
        }
    }
}

class Notifications {
    companion object JniNotifications {
        const val CHANNEL_ID = "Chats"
        private const val NOTIFICATION_ID = 0

        const val SELECT_NOTIFICATION: String = "SELECT_NOTIFICATION"

        /// Key for storing the chat id in the Intent extras field
        const val EXTRAS_NOTIFICATION_ID_KEY: String = "ms.air/notification_id"
        const val EXTRAS_CHAT_ID_KEY: String = "ms.air/chat_id"

        // Category required for the conversation shortcut
        private const val SHORTCUT_CATEGORY_CONVERSATION = "android.shortcut.conversation"


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
            val intent = Intent(context, MainActivity::class.java).apply {
                action = SELECT_NOTIFICATION
                putExtra(EXTRAS_NOTIFICATION_ID_KEY, content.identifier)
                putExtra(EXTRAS_CHAT_ID_KEY, content.chatId?.uuid)
            }

            val pendingIntent = PendingIntent.getActivity(
                context,
                // Unique identifier per intent to ensure that multiple
                // notifications don't overwrite each other's pending intent
                content.identifier.hashCode(),
                intent,
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
                    .addExtras(extras)
                    .build()

            NotificationManagerCompat.from(context)
                .notify(content.identifier, NOTIFICATION_ID, notification)
        }

        @RequiresPermission(Manifest.permission.POST_NOTIFICATIONS)
        private fun showConversationNotification(
            context: Context,
            content: NotificationContent,
            conversation: ConversationNotification,
            chatId: ChatId
        ) {
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
                },
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )

            val extras = Bundle().apply {
                putString(EXTRAS_CHAT_ID_KEY, chatUuid)
            }

            val notification =
                NotificationCompat.Builder(context, CHANNEL_ID)
                    .setContentTitle(content.title)
                    .setContentText(content.body)
                    .setSmallIcon(R.drawable.ic_notification)
                    .setContentIntent(pendingIntent)
                    .setDeleteIntent(deleteIntent)
                    .setDefaults(Notification.DEFAULT_ALL)
                    .setPriority(NotificationManagerCompat.IMPORTANCE_HIGH)
                    .addExtras(extras)
                    .setStyle(buildMessagingStyle(conversation))
                    .setShortcutId(chatUuid)
                    .setLocusId(LocusIdCompat(chatUuid))
                    .setOnlyAlertOnce(!conversation.alert)
                    .build()

            pushConversationShortcut(context, content, chatUuid, conversation)

            NotificationManagerCompat.from(context)
                .notify(content.identifier, NOTIFICATION_ID, notification)
        }

        // Content intent shared by the notification tap target and the conversation shortcut
        //
        // Makes tap routing identical for both.
        private fun buildContentIntent(context: Context, content: NotificationContent): Intent =
            Intent(context, MainActivity::class.java).apply {
                action = SELECT_NOTIFICATION
                putExtra(EXTRAS_NOTIFICATION_ID_KEY, content.identifier)
                putExtra(EXTRAS_CHAT_ID_KEY, content.chatId?.uuid)
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
            for (message in conversation.messages) {
                val sender = personCache.getOrPut(message.senderUuid) {
                    buildParticipantPerson(
                        message.senderUuid,
                        participantsByUuid[message.senderUuid]
                    )
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
        // Shortcut icons are pre-cropped to a circle because launcher and
        // settings surfaces show bitmap icons unmasked.
        private fun decodeAvatarIcon(
            avatarBase64: String?,
            circular: Boolean = false
        ): IconCompat? {
            if (avatarBase64.isNullOrEmpty()) return null
            return try {
                val bytes = Base64.decode(avatarBase64, Base64.DEFAULT)
                val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return null
                if (circular) {
                    IconCompat.createWithBitmap(cropToCircle(bitmap))
                } else {
                    IconCompat.createWithBitmap(bitmap)
                }
            } catch (e: Exception) {
                Log.e(NOTIF_LOGTAG, "Failed to decode avatar", e)
                null
            }
        }

        // Center-crops the bitmap to a circle with transparent corners.
        //
        // TODO: Consider to do this in Rust.
        private fun cropToCircle(source: Bitmap): Bitmap {
            val side = minOf(source.width, source.height)
            val result = createBitmap(side, side)
            val canvas = Canvas(result)
            val paint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
            val radius = side / 2f
            canvas.drawCircle(radius, radius, radius, paint)
            paint.xfermode = PorterDuffXfermode(PorterDuff.Mode.SRC_IN)
            val srcLeft = (source.width - side) / 2
            val srcTop = (source.height - side) / 2
            canvas.drawBitmap(
                source,
                Rect(srcLeft, srcTop, srcLeft + side, srcTop + side),
                Rect(0, 0, side, side),
                paint
            )
            return result
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

                val icon = if (!conversation.isGroup) {
                    decodeAvatarIcon(senderParticipant?.avatar, circular = true)
                        ?: IconCompat.createWithResource(context, R.mipmap.ic_launcher)
                } else {
                    // TODO: Use group avatar
                    IconCompat.createWithResource(context, R.mipmap.ic_launcher)
                }

                // TODO: Do we have to set isGroup here?
                val shortcut = ShortcutInfoCompat.Builder(context, chatUuid)
                    .setLongLived(true)
                    .setShortLabel(shortLabel)
                    .setPerson(person)
                    .setCategories(setOf(SHORTCUT_CATEGORY_CONVERSATION))
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
        }
    }
}
