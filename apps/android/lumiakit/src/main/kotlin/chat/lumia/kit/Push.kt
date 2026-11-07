// Push on the phone (architecture 15.5), the part every receiver shares:
// a push is the instance's generic notice, and this process turns it into
// the message through the core's `notificationContent` (the same call the
// iOS extension makes, under the same writer lock, in-process here since
// Android has no second process) and posts it as a notification whose tap
// opens the conversation. The receivers themselves (UnifiedPush, FCM) and
// the registration live in the app.
package chat.lumia.kit

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import uniffi.lumia_ffi.NotificationContent

/**
 * Where a tapped notification points: the channel and the kind (`dm` or
 * `mention`); the generic notice names none and opens the app where it was.
 */
data class PushTarget(val host: String, val channelId: String, val kind: String) {
    companion object {
        const val EXTRA_INSTANCE = "instance"
        const val EXTRA_CHANNEL = "channel_id"
        const val EXTRA_KIND = "kind"

        /** The target an intent carries, or null for a launch that is not a tap. */
        fun of(intent: Intent?): PushTarget? {
            val channel = intent?.getStringExtra(EXTRA_CHANNEL)?.takeIf { it.isNotEmpty() } ?: return null
            return PushTarget(intent.getStringExtra(EXTRA_INSTANCE) ?: "", channel, intent.getStringExtra(EXTRA_KIND) ?: "dm")
        }
    }
}

object PushRegistrar {
    const val CHANNEL_ID = "messages"
    private const val SUBSCRIPTION_PREFS = "push"

    /** The notification the person tapped last, until the shell opens it. */
    var target by mutableStateOf<PushTarget?>(null)

    /**
     * Sends a subscription to the instance once per value: `unified_push`
     * with the endpoint and the distributor's Web Push keys (the instance
     * encrypts the notice under them), `fcm` with the token. Through the
     * live session when the app is up, else through a client of its own.
     */
    suspend fun subscribe(context: Context, platform: String, value: String, p256dh: String = "", auth: String = "") {
        val prefs = context.getSharedPreferences(SUBSCRIPTION_PREFS, Context.MODE_PRIVATE)
        val saved = AppStore.savedInstance(context) ?: return
        if (prefs.getString(platform, null) == value + p256dh + saved) return
        val session = AppStore.current?.session
        val ok = runCatching {
            if (session != null) session.client.call { pushSubscribe(platform, value, p256dh, auth) }
            else LumiaClient(saved, AppStore.dataDirectory(context)).call { pushSubscribe(platform, value, p256dh, auth) }
        }.isSuccess
        if (ok) prefs.edit().putString(platform, value + p256dh + saved).apply()
    }

    /**
     * A push arrived for `host` (the saved instance when the notice names
     * none): the message behind it from the core, posted as a notification.
     * Nothing while the app is in the foreground, where it shows the message
     * itself. Returns what was posted, for the smoke's record.
     */
    suspend fun received(context: Context, host: String?): NotificationContent? {
        if (AppStore.current?.foregrounded == true) return null
        val saved = AppStore.savedInstance(context) ?: return null
        if (!host.isNullOrEmpty() && !saved.contains(host)) return null
        val content = runCatching {
            LumiaClient(saved, AppStore.dataDirectory(context)).notificationContent(3000u)
        }.getOrNull()
        post(context, content, host ?: "")
        return content
    }

    /** The notification: the message, or the generic notice when the core had none. */
    fun post(context: Context, content: NotificationContent?, host: String) {
        if (Build.VERSION.SDK_INT >= 33 &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "Messages", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Personal messages and mentions"
            },
        )
        val open = context.packageManager.getLaunchIntentForPackage(context.packageName)?.apply {
            addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            putExtra(PushTarget.EXTRA_INSTANCE, host)
            if (content != null) {
                putExtra(PushTarget.EXTRA_CHANNEL, content.channelId)
                putExtra(PushTarget.EXTRA_KIND, content.kind)
            }
        }
        val id = content?.channelId?.hashCode() ?: 0
        val pending = open?.let {
            PendingIntent.getActivity(context, id, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }
        val notification = Notification.Builder(context, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(content?.title ?: "Lumia")
            .setContentText(content?.body ?: "New message")
            .setStyle(Notification.BigTextStyle().bigText(content?.body ?: "New message"))
            .setAutoCancel(true)
            .setContentIntent(pending)
            .setCategory(Notification.CATEGORY_MESSAGE)
            .build()
        manager.notify(id, notification)
    }
}
