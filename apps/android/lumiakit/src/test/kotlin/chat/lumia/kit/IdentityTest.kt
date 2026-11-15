// Setting up private messages, as values: which screen a phone without
// the account's identity gets, when Home's reminder shows, the three-word
// check's fields, the phrase as Restore counts it and as it is printed or
// saved, what Start over says of a key change, and what approving another
// device says of the request pasted and the code it answers.
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

    // A request is base64 on one line; a paste or a share may break it, and
    // anything with other characters in it is not one, said without asking.
    @Test
    fun onlyTextThatCouldBeARequestIsAskedAbout() {
        assertEquals("CgQAAQID+/==", linkRequestOf("  CgQA\nAQID +/==\n"))
        assertTrue(linkRequestWorthAsking("CgQAAQID+/=="))
        assertTrue(linkRequestWorthAsking(" CgQA\r\nAQID "))
        assertFalse(linkRequestWorthAsking(""))
        assertFalse(linkRequestWorthAsking(" \n "))
        assertFalse(linkRequestWorthAsking("hello there!"))
        assertFalse(linkRequestWorthAsking("https://my.lumia.chat/link"))
        assertFalse(linkRequestWorthAsking("CgQA-AQID_"))
        assertTrue(NOT_A_LINK_REQUEST.startsWith("That isn't a link request."))
    }

    // Step 2: a browser is named by the origin it is scoped to, anything else by its label.
    @Test
    fun aRequestSaysWhatItWouldAdd() {
        assertEquals("A device named “MacBook Pro”", linkRequestWho("native", " MacBook Pro ", ""))
        assertEquals("A device with no name", linkRequestWho("native", "", ""))
        assertEquals("A browser on https://my.lumia.chat", linkRequestWho("web", "Safari", "https://my.lumia.chat"))
        assertEquals("A browser", linkRequestWho("web", "", ""))
        assertEquals("A bot named “Ledger”", linkRequestWho("bot", "Ledger", ""))
        assertEquals("A bot", linkRequestWho("bot", "", ""))
        // A kind this build does not know is a device.
        assertEquals("A device named “Pixel”", linkRequestWho("unspecified", "Pixel", ""))
        assertEquals("07070707", linkRequestMeta("native", "MacBook Pro", "07070707-0707-0707-0707-070707070707"))
        assertEquals("Safari · 07070707", linkRequestMeta("web", "Safari", "07070707-0707-0707-0707-070707070707"))
        assertEquals("07070707", linkRequestMeta("web", "", "07070707-0707-0707-0707-070707070707"))
    }

    @Test
    fun theCodeIsReadOutInTwoGroupsOfFour() {
        assertEquals("7KQM 2XRD", linkCodeForReading("7KQM2XRD"))
        assertEquals("7KQM 2XRD", linkCodeForReading(" 7KQM2XRD\n"))
        assertEquals("7 K Q M 2 X R D", linkCodeSpoken("7KQM2XRD"))
        assertEquals("", linkCodeForReading(""))
    }

    @Test
    fun anApprovalTheInstanceRefusedSaysWhyInWords() {
        assertEquals("Your devices changed on another device while this one was approving. Try again.", approveError("CONFLICT", "conflict"))
        assertTrue(approveError("FAILED_PRECONDITION", "failed precondition").startsWith("This account isn't linked"))
        assertTrue(approveError("IDENTITY_PROOF_INVALID", "bad roster").startsWith("The instance refused the new list of devices."))
        assertEquals("Too many tries. Wait a minute, then try again.", approveError("RATE_LIMITED", "slow down"))
        // Anything else, a network failure included, is the failure's own words.
        assertEquals("Not approved: could not reach the instance", approveError(null, "could not reach the instance"))
        assertEquals("Not approved: internal", approveError("INTERNAL", "internal"))
    }

    @Test
    fun anApprovalIsUnderWayUntilItHasACodeOrARefusal() {
        assertTrue(Approval("req").underWay)
        assertFalse(Approval("req", code = "7KQM2XRD").underWay)
        assertFalse(Approval("req", error = "Not approved: offline").underWay)
    }
}
