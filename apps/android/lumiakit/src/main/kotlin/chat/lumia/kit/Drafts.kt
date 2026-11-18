// What is typed into a conversation's composer and not sent (issue #23;
// architecture 13, "Local-first"): the words and the reply they answer,
// which the core keeps in the instance's local store by conversation
// (`Client.draft`, `Client.setDraft`), so a switch to another
// conversation, a quit or a crash loses nothing. Here is when the phone
// reads and keeps one, with no Compose and no thread of its own, so
// `src/test` checks it on the JVM. Not the files waiting in the composer's
// strip (`Media.kt`'s `draftIds` are the core's attachment drafts, another
// thing), and not an edit of a message, which has a field of its own in
// the message's row and never comes here.
package chat.lumia.kit

import kotlin.coroutines.cancellation.CancellationException
import uniffi.lumia_ffi.DraftSummary
import uniffi.lumia_ffi.QuoteSummary

/** How long the words rest before they are kept: a crash or a force-stop loses at most this much typing. */
const val DRAFT_REST_MS = 400L

/**
 * One conversation's draft while its screen is open. [read] and [keep] are
 * the store's two calls for the conversation, [keep] with words that are
 * empty or all blank keeping nothing; [after] runs something once a time
 * has passed and answers with how to take that back. The screen tells it
 * what the composer holds ([changed]), when it goes away or the app is
 * stopped ([flush]) and when the words were sent ([sent]), all from one
 * thread; [after] calls back on it too.
 */
class DraftKeeper(
    private val read: suspend () -> DraftSummary?,
    private val keep: (body: String, reply: QuoteSummary?) -> Unit,
    private val after: (ms: Long, run: () -> Unit) -> () -> Unit,
) {
    // What the composer holds, as last told, and so what the store has or is about to.
    private var body = ""
    private var reply: QuoteSummary? = null
    /** Takes back the save waiting for the words to rest, while one waits. */
    private var waiting: (() -> Unit)? = null
    /** Whether anything was typed or sent here since the screen appeared. */
    private var touched = false

    /**
     * The draft kept for the conversation, to put in the field and the
     * reply bar as the screen appears, at a cold launch too. None when
     * nothing is kept, when it could not be read (said nowhere: the
     * composer is then as it always was), and when something was typed
     * before the store answered, which is newer.
     */
    suspend fun kept(): DraftSummary? {
        val kept = try {
            read()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            null
        }
        if (kept == null || touched) return null
        body = kept.body
        reply = kept.reply
        return kept
    }

    /**
     * The composer now holds [body] and answers [reply]: kept once they
     * have rested [DRAFT_REST_MS], each change starting the wait again, so
     * a burst of keys is one write. Told what it already has (the field as
     * it first draws, a kept draft just put in it), it does nothing; an
     * emptied field is a change like any other, and keeps nothing.
     */
    fun changed(body: String, reply: QuoteSummary?) {
        if (body == this.body && reply == this.reply) return
        touched = true
        this.body = body
        this.reply = reply
        waiting?.invoke()
        waiting = after(DRAFT_REST_MS) {
            waiting = null
            keep(this.body, this.reply)
        }
    }

    /** The screen is going away, or the app was stopped, which Android may end without a word: what waits is kept now. */
    fun flush() {
        val cancel = waiting ?: return
        cancel()
        waiting = null
        keep(body, reply)
    }

    /**
     * The words were sent: nothing is kept from this moment, and a save
     * that was waiting is taken back, so it cannot bring them back later.
     * Words put back in the field after a send that failed are a change,
     * and kept again.
     */
    fun sent() {
        waiting?.invoke()
        waiting = null
        touched = true
        body = ""
        reply = null
        keep("", null)
    }
}

/**
 * The reply as a draft keeps it in a channel whose reply bar has the "@ On"
 * switch: the quote, silent when the switch was turned off, so the bar is
 * put back as it was left ([mentionKept]) and not notifying again.
 */
fun keptReply(quote: QuoteSummary?, mention: Boolean): QuoteSummary? = quote?.copy(silent = !mention)

/** Whether a kept reply's "@ On" switch was on. */
fun mentionKept(kept: QuoteSummary): Boolean = !kept.silent
