// Leaving a session, as rules: the order a sign-out happens in and what
// cannot stop it or do it twice, and what the phone remembers of the push
// subscriptions it sent. The web's order is `signOut` in
// apps/web/src/client/state.ts.
package chat.lumia.kit

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LeavingTest {
    private val done = mutableListOf<String>()

    private suspend fun Leaving.leaveNoting(push: suspend () -> Unit = {}, revoke: suspend () -> Unit = {}): Boolean = leave(
        push = { done += "push"; push() },
        stop = { done += "stop" },
        revoke = { done += "revoke"; revoke() },
        forget = { done += "forget" },
    )

    // The instance keeps a push subscription by device, not by session: it is
    // removed while the session can still ask, before anything else goes.
    @Test
    fun pushIsSeenToFirstThenTheGatewayTheSessionAndWhatWasHeld() = runBlocking {
        val leaving = Leaving()
        assertTrue(leaving.leaveNoting())
        assertEquals(listOf("push", "stop", "revoke", "forget"), done)
        assertFalse(leaving.underWay)
    }

    // No network, or a session the instance has already ended: the phone is signed out all the same.
    @Test
    fun aFailureToUnsubscribeOrToRevokeDoesNotStopTheSignOut() = runBlocking {
        val leaving = Leaving()
        assertTrue(leaving.leaveNoting(push = { error("no route to host") }, revoke = { error("session revoked") }))
        assertEquals(listOf("push", "stop", "revoke", "forget"), done)
        assertFalse(leaving.underWay)
    }

    @Test
    fun aSecondLeavingWhileOneIsUnderWayDoesNothing() = runBlocking {
        val leaving = Leaving()
        val held = CompletableDeferred<Unit>()
        val first = launch(start = CoroutineStart.UNDISPATCHED) { leaving.leaveNoting(push = { held.await() }) }
        assertTrue(leaving.underWay)
        // A second tap, or the snapshot refused by the sign-out itself.
        assertFalse(leaving.leaveNoting())
        assertEquals(listOf("push"), done)
        held.complete(Unit)
        first.join()
        assertEquals(listOf("push", "stop", "revoke", "forget"), done)
        assertFalse(leaving.underWay)
        // Over, it can happen again: the next session's.
        assertTrue(leaving.leaveNoting())
    }

    // The screen that asked goes away, or the refusal arrived on the gateway's
    // own job, which stopping the gateway cancels: tokens left behind would
    // open the shell on a dead session at the next launch.
    @Test
    fun aLeavingIsSeenThroughWhenItsCallerIsCancelled() = runBlocking {
        val leaving = Leaving()
        val unsubscribing = CompletableDeferred<Unit>()
        val revoking = CompletableDeferred<Unit>()
        val caller = launch(start = CoroutineStart.UNDISPATCHED) {
            leaving.leaveNoting(push = { unsubscribing.await() }, revoke = { revoking.await() })
        }
        val cancelled = launch(start = CoroutineStart.UNDISPATCHED) { caller.cancelAndJoin() }
        assertEquals(listOf("push"), done)
        unsubscribing.complete(Unit)
        revoking.complete(Unit)
        cancelled.join()
        assertTrue(caller.isCancelled)
        assertEquals(listOf("push", "stop", "revoke", "forget"), done)
        assertFalse(leaving.underWay)
    }

    @Test
    fun theSameValueForTheSameInstanceIsTheSameMarkAndAnythingElseIsNot() {
        val mark = pushMark("https://chat.example.com", "https://ntfy.sh/up1", "key")
        assertEquals(mark, pushMark("https://chat.example.com", "https://ntfy.sh/up1", "key"))
        // A new endpoint, new keys for the old one, and the same endpoint for another instance are each sent.
        assertFalse(mark == pushMark("https://chat.example.com", "https://ntfy.sh/up2", "key"))
        assertFalse(mark == pushMark("https://chat.example.com", "https://ntfy.sh/up1", "other"))
        assertFalse(mark == pushMark("https://other.example.com", "https://ntfy.sh/up1", "key"))
        // Where one part ends and the next begins is part of the mark.
        assertFalse(pushMark("https://chat.example.com", "ab", "c") == pushMark("https://chat.example.com", "a", "bc"))
    }

    @Test
    fun aSignOutTakesBackWhatThePhoneSubscribedAsOnThatInstance() {
        val here = "https://chat.example.com"
        assertEquals(emptyList<String>(), subscribedOn(emptyMap(), here))
        assertEquals(listOf("unified_push"), subscribedOn(mapOf("unified_push" to pushMark(here, "https://ntfy.sh/up1", "key")), here))
        assertEquals(listOf("fcm"), subscribedOn(mapOf("fcm" to pushMark(here, "token")), here))
        // Both, should the phone ever have sent both; never one sent to another instance,
        // nor to one whose address only begins the same.
        val marks = mapOf(
            "unified_push" to pushMark(here, "https://ntfy.sh/up1", "key"),
            "fcm" to pushMark(here, "token"),
            "apns" to pushMark("https://other.example.com", "token"),
        )
        assertEquals(listOf("fcm", "unified_push"), subscribedOn(marks, here))
        assertEquals(listOf("apns"), subscribedOn(marks, "https://other.example.com"))
        assertEquals(emptyList<String>(), subscribedOn(marks, "https://chat.example.co"))
    }

    @Test
    fun aPushIsLookedIntoWhenItNamesTheSavedInstanceOrNone() {
        assertTrue(pushIsFor("https://chat.example.com", "chat.example.com"))
        assertTrue(pushIsFor("https://chat.example.com", null))
        assertTrue(pushIsFor("https://chat.example.com", ""))
        assertFalse(pushIsFor("https://chat.example.com", "other.example.com"))
        // No instance saved: nothing to ask.
        assertFalse(pushIsFor(null, "chat.example.com"))
        assertFalse(pushIsFor(null, null))
    }
}
