// What is typed and not sent (issue #23): when the phone reads a
// conversation's draft and when it keeps one, and the order of a send
// (kept, sent, and only then cleared; kept still when the send throws),
// against a store that is a map and rests that pass when the test says so; and once against the
// core itself (the library `scripts/core.sh` builds for this machine),
// with no instance, where a second client on the same files is the next
// launch.
package chat.lumia.kit

import java.nio.file.Files
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.DraftSummary
import uniffi.lumia_ffi.QuoteSummary

private const val HERE = "01890000-0000-7000-8000-000000000031"
private const val THERE = "01890000-0000-7000-8000-000000000032"

private val quote = QuoteSummary("01890000-0000-7000-8000-000000000041", "01890000-0000-7000-8000-000000000051", "shall we?", false, false)

/** The store's two calls as a map by conversation, as the core answers them, with every write as it was made. */
private class Store {
    val kept = mutableMapOf<String, DraftSummary>()
    val writes = mutableListOf<Pair<String, QuoteSummary?>>()
    var broken = false

    fun read(id: String): DraftSummary? {
        if (broken) throw IllegalStateException("the store cannot be read")
        return kept[id]
    }

    fun keep(id: String, body: String, reply: QuoteSummary?) {
        writes += body to reply
        if (body.isBlank()) kept.remove(id) else kept[id] = DraftSummary(body, reply)
    }
}

/** The rests a keeper asked for, which pass when the test says so. */
private class Rests {
    private val waiting = mutableListOf<Pair<Long, () -> Unit>>()
    val after: (Long, () -> Unit) -> () -> Unit = { ms, run ->
        val rest = ms to run
        waiting += rest
        val takeBack: () -> Unit = { waiting.remove(rest) }
        takeBack
    }
    val asked: List<Long> get() = waiting.map { it.first }

    fun pass() {
        val due = waiting.toList()
        waiting.clear()
        due.forEach { it.second() }
    }
}

private fun keeper(store: Store, rests: Rests, id: String = HERE) =
    DraftKeeper({ store.read(id) }, { body, reply -> store.keep(id, body, reply) }, rests.after)

private fun DraftKeeper.read(): DraftSummary? = runBlocking { kept() }

class DraftsTest {
    @Test
    fun aDraftIsReadBackWhenItsConversationAppearsAgain() {
        val store = Store()
        val rests = Rests()
        val first = keeper(store, rests)
        assertNull(first.read())
        first.changed("Half a thought", quote)
        rests.pass()
        // The screen gone and back: another keeper over the same store.
        assertEquals(DraftSummary("Half a thought", quote), keeper(store, rests).read())
        // Each conversation has its own.
        assertNull(keeper(store, rests, THERE).read())
    }

    @Test
    fun aBurstOfKeysIsOneWriteOnceTheyRest() {
        val store = Store()
        val rests = Rests()
        val k = keeper(store, rests)
        k.changed("H", null)
        k.changed("He", null)
        k.changed("Hey", null)
        // Nothing yet, and one rest waited for: each key took the one before back.
        assertEquals(emptyList<Pair<String, QuoteSummary?>>(), store.writes)
        assertEquals(listOf(DRAFT_REST_MS), rests.asked)
        rests.pass()
        assertEquals(listOf<Pair<String, QuoteSummary?>>("Hey" to null), store.writes)
        // Told the same again, the caret moved or the screen drawn again: nothing.
        k.changed("Hey", null)
        assertEquals(emptyList<Long>(), rests.asked)
        // The reply alone is a change.
        k.changed("Hey", quote)
        rests.pass()
        assertEquals(listOf("Hey" to null, "Hey" to quote), store.writes)
    }

    @Test
    fun theFieldAsItFirstDrawsAndAKeptDraftPutInItAreNotChanges() {
        val store = Store()
        val rests = Rests()
        store.kept[HERE] = DraftSummary("Kept", quote)
        val k = keeper(store, rests)
        // The screen draws its empty field before the store has answered: that must not empty the store.
        k.changed("", null)
        assertEquals(emptyList<Long>(), rests.asked)
        assertEquals(DraftSummary("Kept", quote), k.read())
        // And then holds what was kept.
        k.changed("Kept", quote)
        assertEquals(emptyList<Long>(), rests.asked)
        k.flush()
        assertEquals(emptyList<Pair<String, QuoteSummary?>>(), store.writes)
    }

    @Test
    fun leavingKeepsWhatWaitsAtOnce() {
        val store = Store()
        val rests = Rests()
        val k = keeper(store, rests)
        k.changed("On my way out", quote)
        k.flush()
        assertEquals(listOf<Pair<String, QuoteSummary?>>("On my way out" to quote), store.writes)
        // The rest it was waiting for was taken back, and with nothing waiting, leaving again writes nothing.
        assertEquals(emptyList<Long>(), rests.asked)
        rests.pass()
        k.flush()
        assertEquals(1, store.writes.size)
    }

    @Test
    fun aSendClearsTheDraftAndTakesBackTheSaveThatWaited() {
        val store = Store()
        val rests = Rests()
        val k = keeper(store, rests)
        k.changed("Sent", quote)
        rests.pass()
        k.changed("Sent words", quote)
        k.sent()
        // Cleared at once, and the save of "Sent words" is not waiting to bring them back.
        assertNull(store.kept[HERE])
        assertEquals(emptyList<Long>(), rests.asked)
        rests.pass()
        k.flush()
        assertEquals(listOf("Sent" to quote, "" to null), store.writes)
        // The field the send emptied is what the keeper already has.
        k.changed("", null)
        assertEquals(emptyList<Long>(), rests.asked)
        // What is typed after it is a change like any other: kept again.
        k.changed("Sent words", quote)
        rests.pass()
        assertEquals(DraftSummary("Sent words", quote), store.kept[HERE])
    }

    @Test
    fun aSendHasItsWordsKeptUntilItHasAnswered() {
        val store = Store()
        val rests = Rests()
        val k = keeper(store, rests)
        // Typed and sent before the words had rested: nothing of them is in the store yet.
        k.changed("On their way", quote)
        assertNull(store.kept[HERE])
        var during: DraftSummary? = null
        val answer = runBlocking {
            k.sending {
                // While the core has the send, the store has the draft: a phone that dies here has the words.
                during = store.kept[HERE]
                "sent"
            }
        }
        assertEquals("sent", answer)
        assertEquals(DraftSummary("On their way", quote), during)
        // Answered without error: cleared, in that order, and nothing waits to bring the words back.
        assertNull(store.kept[HERE])
        assertEquals(listOf("On their way" to quote, "" to null), store.writes)
        rests.pass()
        assertEquals(2, store.writes.size)
    }

    @Test
    fun aSendThatThrowsLeavesTheDraftAsItWas() {
        val store = Store()
        val rests = Rests()
        val k = keeper(store, rests)
        k.changed("Not gone", quote)
        val thrown = runCatching { runBlocking { k.sending<Unit> { throw IllegalStateException("the gateway loop has stopped") } } }.exceptionOrNull()
        // The failure is the screen's to say, and the words are still the conversation's draft.
        assertEquals("the gateway loop has stopped", thrown?.message)
        assertEquals(DraftSummary("Not gone", quote), store.kept[HERE])
        assertEquals(listOf<Pair<String, QuoteSummary?>>("Not gone" to quote), store.writes)
        // The field still holds them, which is nothing new to the keeper; and the next launch has them.
        k.changed("Not gone", quote)
        assertEquals(emptyList<Long>(), rests.asked)
        assertEquals(DraftSummary("Not gone", quote), keeper(store, rests).read())
        // Sent again, and through this time: cleared.
        runBlocking { k.sending { } }
        assertNull(store.kept[HERE])
    }

    @Test
    fun aSendIsNotCutShortByItsScreenGoingAway() {
        val store = Store()
        val rests = Rests()
        val k = keeper(store, rests)
        k.changed("Sent as the screen left", null)
        runBlocking {
            val asked = CompletableDeferred<Unit>()
            val answered = CompletableDeferred<Unit>()
            val send = launch {
                k.sending {
                    asked.complete(Unit)
                    answered.await()
                }
            }
            asked.await()
            // The conversation is left while the core has the send, which goes through all the same.
            send.cancel()
            answered.complete(Unit)
            send.join()
        }
        // The words went, so their draft does too: left behind, it would have them sent a second time.
        assertNull(store.kept[HERE])
        assertEquals(listOf<Pair<String, QuoteSummary?>>("Sent as the screen left" to null, "" to null), store.writes)
    }

    @Test
    fun theFieldLetsGoOfWhatWasSentAndOnlyOfThat() {
        assertEquals("", afterSend("Hello", "Hello"))
        assertEquals("", afterSend("Hello\n", "Hello\n"))
        // Typed on while the send was on its way: those words are not the message's, and stay.
        assertEquals("are you there?", afterSend("Hello", "Hello are you there?"))
        assertEquals("next line", afterSend("Hello", "Hello\nnext line"))
        // Changed in some other way meanwhile: left as it is.
        assertEquals("Hullo", afterSend("Hello", "Hullo"))
        assertEquals("", afterSend("Hello", ""))
    }

    @Test
    fun anEmptiedFieldKeepsNothing() {
        val store = Store()
        val rests = Rests()
        store.kept[HERE] = DraftSummary("Kept", quote)
        val k = keeper(store, rests)
        assertEquals("Kept", k.read()?.body)
        k.changed("", quote)
        rests.pass()
        assertNull(store.kept[HERE])
    }

    @Test
    fun whatWasTypedBeforeTheStoreAnsweredIsNotPutOver() {
        val store = Store()
        val rests = Rests()
        store.kept[HERE] = DraftSummary("Older", null)
        val k = keeper(store, rests)
        k.changed("N", null)
        assertNull(k.read())
        rests.pass()
        assertEquals(DraftSummary("N", null), store.kept[HERE])
    }

    @Test
    fun aStoreThatCannotBeReadIsNoDraft() {
        val store = Store()
        val rests = Rests()
        store.kept[HERE] = DraftSummary("Kept", null)
        store.broken = true
        val k = keeper(store, rests)
        assertNull(k.read())
        // The composer is as it always was, and what is typed is still handed over.
        k.changed("Typed anyway", null)
        rests.pass()
        assertEquals(listOf<Pair<String, QuoteSummary?>>("Typed anyway" to null), store.writes)
    }

    @Test
    fun aKeptReplyHasItsMentionSwitchAsItWasLeft() {
        assertNull(keptReply(null, true))
        val on = keptReply(quote, true)!!
        assertEquals(quote, on)
        assertTrue(mentionKept(on))
        val off = keptReply(quote, false)!!
        assertEquals(quote.copy(silent = true), off)
        assertFalse(mentionKept(off))
    }

    @Test
    fun aDraftKeptByTheCoreIsThereAtTheNextLaunch() {
        // No instance is asked anything: the address only names the folder the store is in.
        val address = "http://localhost:9"
        val dir = Files.createTempDirectory("lumia-draft").toFile()
        try {
            val rests = Rests()
            val first = LumiaClient(address, dir)
            val typing = DraftKeeper({ first.now { draft(HERE) } }, { body, reply -> first.now { setDraft(HERE, body, reply) } }, rests.after)
            assertNull(typing.read())
            typing.changed("Half a thou", null)
            rests.pass()
            typing.changed("Half a thought\n", quote)
            typing.flush()
            // The process gone (a quit, a crash, a force-stop): another client on the same files.
            val second = LumiaClient(address, dir)
            val back = DraftKeeper({ second.now { draft(HERE) } }, { body, reply -> second.now { setDraft(HERE, body, reply) } }, rests.after)
            assertEquals(DraftSummary("Half a thought\n", quote), back.read())
            assertNull(second.now { draft(THERE) })
            // Sent: a third launch finds nothing.
            runBlocking { back.sending { } }
            assertNull(LumiaClient(address, dir).now { draft(HERE) })
        } finally {
            dir.deleteRecursively()
        }
    }
}
