// Leaving a session, as rules with nothing of Android or Compose in them
// (tested in lumiakit/src/test, as Timeline.kt's and Preferences.kt's
// are): the order a session is left in, whether the person chose to sign
// out or the instance ended it, and what the phone remembers of the push
// subscriptions it sent, which a leaving takes back and forgets.
package chat.lumia.kit

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext

/**
 * A session's leaving: one at a time, in order, and to the end.
 *
 * The order is the web's `signOut`: what the phone has to say about push
 * while the session can still say it, then the gateway stopped, then the
 * session revoked and its tokens forgotten, then what the phone held of
 * the account dropped. The instance keeps a push subscription by device
 * and not by session, so one left behind would go on sending this phone
 * notices for an account it has signed out of.
 *
 * Called on one thread (the session's, which is the main one); another
 * may read [underWay].
 */
class Leaving {
    /** Whether a leaving is under way: nothing is subscribed, shown or left a second time meanwhile. */
    @Volatile
    var underWay = false
        private set

    /**
     * Leaves, unless a leaving is already under way: then nothing is done
     * and the answer is false, so a second tap, or the instance refusing a
     * call the sign-out itself made stale, does not do any of it twice.
     *
     * [push] and [revoke] may fail (no network, a session already gone) and
     * the rest still happens: the phone is signed out either way. And it is
     * seen through even when the caller is cancelled part-way, by the
     * screen that asked going away or by [stop] ending the gateway's job
     * when the refusal arrived on it: tokens left behind would open the
     * shell on a dead session at the next launch.
     */
    suspend fun leave(push: suspend () -> Unit, stop: () -> Unit, revoke: suspend () -> Unit, forget: () -> Unit): Boolean {
        if (underWay) return false
        underWay = true
        try {
            withContext(NonCancellable) {
                runCatching { push() }
                stop()
                runCatching { revoke() }
                forget()
            }
        } finally {
            underWay = false
        }
        return true
    }
}

// Push subscriptions (architecture 15.5)

/**
 * What the phone keeps of a subscription it has sent, by platform
 * (`unified_push`, `fcm`): the instance it went to and the value, an
 * endpoint with the key the instance encrypts under, or a token. The same
 * value for the same instance is not sent twice; anything else is.
 */
fun pushMark(instance: String, value: String, p256dh: String = ""): String = "$instance\n$value\n$p256dh"

/**
 * The platforms this phone remembers subscribing as on [instance], from
 * its marks by platform: what a sign-out there asks the instance to drop.
 * One, as a rule: the phone registers a distributor or FCM, not both.
 */
fun subscribedOn(marks: Map<String, String>, instance: String): List<String> =
    marks.filterValues { it.startsWith("$instance\n") }.keys.sorted()

/**
 * Whether a push that arrived is one to look into: there is a saved
 * instance, and the notice names it or names none (UnifiedPush's plain
 * notice from an instance that does not say).
 */
fun pushIsFor(saved: String?, host: String?): Boolean =
    saved != null && (host.isNullOrEmpty() || saved.contains(host))
