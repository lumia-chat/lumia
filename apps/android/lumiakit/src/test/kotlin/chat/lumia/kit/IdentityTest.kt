// Setting up private messages, as values: which screen a phone without
// the account's identity gets, when Home's reminder shows, the three-word
// check's fields, the phrase as Restore counts it and as it is printed or
// saved, and what Start over says of a key change.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class IdentityTest {
    private val words = (1..24).map { "w$it" }

    // An account with no identity is set up on this phone; one bound to
    // this phone's is ready; one bound elsewhere is linked, restored or
    // started over, never given a second.
    @Test
    fun aPhoneWithoutTheAccountsIdentityIsSetUpOnlyWhenTheAccountHasNone() {
        assertEquals(PrivateMessages.SET_UP, privateMessagesFor(held = null, bound = null))
        assertEquals(PrivateMessages.SET_UP, privateMessagesFor(held = null, bound = ""))
        // This phone's own, binding the account to it.
        assertEquals(PrivateMessages.SET_UP, privateMessagesFor(held = "ab12", bound = null))
        assertEquals(PrivateMessages.READY, privateMessagesFor(held = "ab12", bound = "ab12"))
        assertEquals(PrivateMessages.READY, privateMessagesFor(held = "AB12", bound = "ab12"))
        assertEquals(PrivateMessages.ELSEWHERE, privateMessagesFor(held = null, bound = "ab12"))
        assertEquals(PrivateMessages.ELSEWHERE, privateMessagesFor(held = "cd34", bound = "ab12"))
    }

    @Test
    fun homeSaysWhatThePhoneNeedsAndNothingWhenItIsReady() {
        assertNull(setupNotice(hasIdentity = true, need = PrivateMessages.READY))
        assertEquals("Private messages aren't set up on this phone yet.", setupNotice(hasIdentity = true, need = PrivateMessages.SET_UP))
        assertTrue(setupNotice(hasIdentity = false, need = PrivateMessages.ELSEWHERE)!!.startsWith("Your account has private messages on another device."))
        // Not asked yet: an identity held is taken as ready, none held is said plainly.
        assertNull(setupNotice(hasIdentity = true, need = null))
        assertEquals("Private messages need this phone.", setupNotice(hasIdentity = false, need = null))
    }

    // "In a week" puts Home's reminder off for seven days, and then it is back.
    @Test
    fun theReminderIsPutOffForSevenDays() {
        val now = 1_700_000_000_000L
        val until = phraseSnoozedUntil(now)
        assertEquals(7L * 24 * 60 * 60 * 1000, until - now)
        assertTrue(phraseReminderShows(unconfirmed = true, snoozedUntil = null, now = now))
        assertFalse(phraseReminderShows(unconfirmed = true, snoozedUntil = until, now = now))
        assertFalse(phraseReminderShows(unconfirmed = true, snoozedUntil = until, now = until - 1))
        assertTrue(phraseReminderShows(unconfirmed = true, snoozedUntil = until, now = until))
        // A confirmed phrase is never reminded of.
        assertFalse(phraseReminderShows(unconfirmed = false, snoozedUntil = null, now = now))
    }

    @Test
    fun aKeptSnoozeThisBuildCannotReadIsNone() {
        assertEquals(42L, phraseSnoozeOf("42"))
        assertEquals(42L, phraseSnoozeOf(" 42\n"))
        assertNull(phraseSnoozeOf(null))
        assertNull(phraseSnoozeOf("next week"))
    }

    // Restore counts the words however they were typed or pasted, and turns on at exactly 24.
    @Test
    fun restoreCountsTheWordsAndTurnsOnAtTwentyFour() {
        assertEquals("0 of 24", phraseCount(""))
        assertEquals("3 of 24", phraseCount("  Orbit cactus\n\ttimber "))
        assertEquals(listOf("orbit", "cactus", "timber"), phraseWords("  Orbit cactus\n\ttimber "))
        assertFalse(restoreReady(words.take(23).joinToString(" ")))
        assertTrue(restoreReady(words.joinToString("\n")))
        assertFalse(restoreReady((words + "extra").joinToString(" ")))
        assertEquals(words.joinToString(" "), phraseOf(words.joinToString("  \n ")))
    }

    // Two columns read down: 1 to 12, then 13 to 24.
    @Test
    fun theWordsAreNumberedInTwoColumnsReadDown() {
        val (first, second) = phraseColumns(words)
        assertEquals((1..12).toList(), first.map { it.first })
        assertEquals((13..24).toList(), second.map { it.first })
        assertEquals("w1", first[0].second)
        assertEquals("w13", second[0].second)
    }

    // The file has the warning and the words one a line, in order.
    @Test
    fun theSavedFileIsTheWarningAndTheWordsInOrder() {
        val doc = phraseDocument(words)
        val lines = doc.lines()
        assertEquals(PHRASE_DOCUMENT_TITLE, lines[0])
        assertEquals(PHRASE_WARNING, lines[2])
        assertEquals("1. w1", lines[4])
        assertEquals("24. w24", lines[27])
    }

    // Each field holds a word as the wordlist has them; Confirm waits for all three.
    @Test
    fun theCheckWaitsForAWordInEveryField() {
        var quiz = PhraseQuiz(listOf(4, 11, 19))
        assertFalse(quiz.ready)
        quiz = quiz.typed(0, " Velvet ").typed(1, "fa")
        assertEquals(listOf("velvet", "fa", ""), quiz.words)
        assertFalse(quiz.ready)
        quiz = quiz.typed(2, "clever")
        assertTrue(quiz.ready)
        assertFalse(PhraseQuiz(emptyList()).ready)
    }

    // Completions show under a field being typed in, and go once it holds the only word it could be.
    @Test
    fun completionsAreOfferedUntilTheFieldHoldsTheirOnlyWord() {
        val quiz = PhraseQuiz(listOf(4, 11, 19)).typed(1, "fa").typed(2, "fabric")
        assertEquals(emptyList<String>(), quiz.offered(0, listOf("abandon")))
        assertEquals(listOf("fabric", "face", "faculty"), quiz.offered(1, listOf("fabric", "face", "faculty")))
        assertEquals(emptyList<String>(), quiz.offered(2, listOf("fabric")))
        // A word that begins others still offers them.
        assertEquals(listOf("face", "faculty"), quiz.typed(2, "face").offered(2, listOf("face", "faculty")))
        assertEquals("Words starting with fa", completionsLabel("fa"))
    }

    // Only someone who talked privately with the old identity pinned it.
    @Test
    fun startOverSaysWhoWillSeeTheKeyChange() {
        val me = "me"
        assertEquals(0, privateContacts(emptyList(), me))
        assertEquals(2, privateContacts(listOf(listOf(me, "ada"), listOf(me, "ada", "grace")), me))
        assertEquals("Nobody will see a key change: you have no private conversations yet.", keyChangeWords(0))
        assertEquals("The 1 person you talk to privately will see that your key changed.", keyChangeWords(1))
        assertEquals("The 3 people you talk to privately will see that your key changed.", keyChangeWords(3))
    }
}
