// Push on the phone (architecture 15.5), the part every receiver shares:
// a push is the instance's generic notice, and this process turns it into
// the message through the core's `notificationContent` (the same call the
// iOS extension makes, under the same writer lock, in-process here since
// Android has no second process) and posts it as a notification whose tap
// opens the conversation. The receivers themselves (UnifiedPush, FCM) and
// the registration live in the app. A phone that is signed out subscribes
// to nothing and shows nothing: leaving a session takes the subscription
// back from the instance, forgets it here and gives up the phone's own
// registration with the distributor or FCM (`left`).
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
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.NotificationContent
import uniffi.lumia_ffi.ReadStateSummary

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
     * How the phone's own registrations for push are given up: the one with
     * the UnifiedPush distributor and FCM's token. The app's to say, the
     * libraries being its (`app/.../push`, set by the application); `left`
     * calls it for every session left.
     */
    @Volatile
    var unregister: (suspend (Context) -> Unit)? = null

    /** One at a time: a subscription is not sent while a leaving takes the last one back. */
    private val sending = Mutex()

    /** Whether the session the app holds is over or on its way out: nothing is subscribed or shown for it. */
    private fun leftOrLeaving(): Boolean = AppStore.current?.session?.let { !it.signedIn || it.leaving } ?: false

    private fun marks(context: Context) = context.getSharedPreferences(SUBSCRIPTION_PREFS, Context.MODE_PRIVATE)

    /**
     * Sends a subscription to the instance once per value: `unified_push`
     * with the endpoint and the distributor's Web Push keys (the instance
     * encrypts the notice under them), `fcm` with the token. Through the
     * live session when the app is up, else through a client of its own.
     * Nothing for a phone that is signed out or signing out, and what was
     * sent is forgotten when the session is left (`left`), so the next
     * sign-in sends it again.
     */
    suspend fun subscribe(context: Context, platform: String, value: String, p256dh: String = "", auth: String = "") {
        val saved = AppStore.savedInstance(context) ?: return
        sending.withLock {
            val mark = pushMark(saved, value, p256dh)
            if (marks(context).getString(platform, null) == mark) return
            if (leftOrLeaving()) return
            val session = AppStore.current?.session
            val ok = runCatching {
                val client = session?.client ?: LumiaClient(saved, AppStore.dataDirectory(context))
                if (!client.signedIn) return
                client.call { pushSubscribe(platform, value, p256dh, auth) }
            }.isSuccess
            if (ok) marks(context).edit().putString(platform, mark).apply()
        }
    }

    /**
     * The session is being left, and push goes first (`Leaving`). A
     * sign-out the person [chosen] asks the instance, through the session
     * while it can still ask, to drop each subscription this phone
     * remembers sending it: the instance keeps them by device, not by
     * session, and would go on sending this phone notices for the account.
     * A session the instance ended can ask nothing. Either way, and whether
     * or not the instance was reached, what the phone remembered sending is
     * forgotten: the next sign-in, as this account or another, subscribes
     * again, which replaces what the instance holds for this device. Then
     * the phone's own registrations go, the distributor's and FCM's
     * (`unregister`), as the web drops the browser's own subscription: the
     * endpoint or token the account's notices went to is not kept alive
     * for nobody, and the next sign-in registers for one of its own. And
     * the notifications the phone is showing go, being the account's
     * messages, with the one tapped last if the shell had not opened it yet.
     *
     * A refusal or no network is left at that and stops nothing: a
     * subscription the instance could not be asked to drop (the instance
     * itself drops a device's when its last session ends) sends pushes
     * that `received` shows nothing for. Nor does a distributor or FCM
     * that cannot be told.
     */
    suspend fun left(context: Context, client: LumiaClient, chosen: Boolean) {
        sending.withLock {
            val marks = marks(context)
            if (chosen) {
                val sent = marks.all.mapNotNull { (platform, mark) -> (mark as? String)?.let { platform to it } }.toMap()
                for (platform in subscribedOn(sent, client.baseUrl)) runCatching { client.call { pushUnsubscribe(platform) } }
            }
            // Written before going on: a mark that outlived its subscription would keep the next sign-in from subscribing.
            withContext(Dispatchers.IO) { marks.edit().clear().commit() }
        }
        runCatching { unregister?.invoke(context) }
        runCatching { context.getSystemService(NotificationManager::class.java).cancelAll() }
        target = null
    }

    /**
     * A push arrived for `host` (the saved instance when the notice names
     * none): the message behind it from the core, posted as a notification.
     * Nothing while the app is in the foreground, where it shows the message
     * itself, and nothing at all for a phone that is signed out, which a
     * push can still reach (a subscription the instance could not be asked
     * to drop, or one on its way as the sign-out happened): no notice, not
     * the generic one either. Returns what was posted, for the smoke's record.
     */
    suspend fun received(context: Context, host: String?, badge: Int? = null): NotificationContent? {
        if (AppStore.current?.foregrounded == true) return null
        val saved = AppStore.savedInstance(context)
        if (saved == null || !pushIsFor(saved, host)) return null
        if (leftOrLeaving()) return null
        val content = runCatching {
            val client = LumiaClient(saved, AppStore.dataDirectory(context))
            if (!client.signedIn) return null
            client.notificationContent(3000u)
        }.getOrNull()
        post(context, content, host ?: "", badge)
        return content
    }

    /**
     * A read push (`{"kind": "read", "badge": n}`, architecture 6.6 and
     * 15.5): the account read a conversation further on another device.
     * It shows nothing: the counts are read again, from the session the app
     * holds when there is one (its snapshot, which `readsChanged` settles
     * from) or else from a snapshot of a client of its own, and the
     * delivered notifications of conversations now read go, the rest
     * carrying the badge. Nothing for a push that is not this phone's, or
     * for a phone signed out or signing out. Returns the conversations
     * whose notifications went, for the smoke's record.
     */
    suspend fun readElsewhere(context: Context, host: String?, badge: Int?): Set<String> {
        val saved = AppStore.savedInstance(context)
        if (saved == null || !pushIsFor(saved, host)) return emptySet()
        if (leftOrLeaving()) return emptySet()
        val session = AppStore.current?.session
        if (session != null && session.signedIn) {
            withContext(Dispatchers.Main) { session.refreshSnapshot() }
            return settle(context, session.readStates.values, session.badge)
        }
        val states = runCatching {
            val client = LumiaClient(saved, AppStore.dataDirectory(context))
            if (!client.signedIn) return emptySet()
            client.call { snapshot() }.readStates
        }.getOrNull()
        return settle(context, states ?: emptyList(), badge ?: states?.let { badgeOf(it) } ?: return emptySet())
    }

    /**
     * The delivered notifications as the counts now stand: those of
     * conversations with nothing unread go (each is tagged with its
     * channel), all of them when nothing that notifies is unread, and the
     * rest carry [badge] as their number, which is the count a launcher
     * shows where it shows one. Android has no badge of its own to set: a
     * launcher counts what the notifications say. Returns the
     * conversations whose notifications went.
     */
    fun settle(context: Context, states: Collection<ReadStateSummary>, badge: Int): Set<String> {
        val manager = runCatching { context.getSystemService(NotificationManager::class.java) }.getOrNull() ?: return emptySet()
        val ours = runCatching { manager.activeNotifications.filter { it.id == NOTIFICATION_ID } }.getOrDefault(emptyList())
        if (ours.isEmpty()) return emptySet()
        val read = if (badge == 0) ours.mapNotNull { it.tag }.toSet() else readNow(states, ours.mapNotNull { it.tag })
        for (n in ours) {
            if (badge == 0 || n.tag in read) {
                manager.cancel(n.tag, n.id)
            } else if (n.notification.number != badge) {
                val again = Notification.Builder.recoverBuilder(context, n.notification).setNumber(badge).setOnlyAlertOnce(true).build()
                manager.notify(n.tag, n.id, again)
            }
        }
        return read
    }

    /** Every notification the app posts has this id, and its conversation's channel as its tag (the generic notice: [GENERIC_TAG]). */
    private const val NOTIFICATION_ID = 1
    private const val GENERIC_TAG = "lumia"

    /** The notification: the message, or the generic notice when the core had none, numbered with the account's [badge] when the push carried one. */
    fun post(context: Context, content: NotificationContent?, host: String, badge: Int? = null) {
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
        val builder = Notification.Builder(context, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(content?.title ?: "Lumia")
            .setContentText(content?.body ?: "New message")
            .setStyle(Notification.BigTextStyle().bigText(content?.body ?: "New message"))
            .setAutoCancel(true)
            .setContentIntent(pending)
            .setCategory(Notification.CATEGORY_MESSAGE)
        if (badge != null && badge > 0) builder.setNumber(badge)
        // Tagged with its conversation, so reading that conversation, here or elsewhere, takes it away (`settle`).
        manager.notify(content?.channelId?.ifEmpty { null } ?: GENERIC_TAG, NOTIFICATION_ID, builder.build())
    }
}
