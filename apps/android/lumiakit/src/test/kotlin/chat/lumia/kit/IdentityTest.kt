// Setting up private messages, as values: which screen a phone without
// the account's identity gets, when Home's reminder shows, the three-word
// check's fields, the phrase as Restore counts it and as it is printed or
// saved, what Start over says of a key change, what approving another
// device says of the request pasted and the code it answers, and linking
// by prompt: the new device's stages and the prompt another device shows.
package chat.lumia.kit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import java.time.ZoneId
import org.junit.Test
import uniffi.lumia_ffi.DeviceSummary

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

    // Linking by prompt (identity spec 4.2.1), as the new device.

    @Test
    fun theNewDeviceReadsTheCoresStagesAndAsksAgainOnlyWhileAnotherDeviceMayMoveIt() {
        assertEquals(LinkStage.WAITING, linkStageOf("waiting"))
        assertEquals(LinkStage.COMPARE, linkStageOf("compare"))
        assertEquals(LinkStage.LINKED, linkStageOf("linked"))
        assertEquals(LinkStage.DECLINED, linkStageOf("declined"))
        assertEquals(LinkStage.EXPIRED, linkStageOf("expired"))
        // A stage this build does not know waits, and asks again.
        assertEquals(LinkStage.WAITING, linkStageOf("thinking"))
        assertEquals(setOf(LinkStage.WAITING, LinkStage.COMPARE), LinkStage.entries.filter { it.polls }.toSet())
        assertEquals(setOf(LinkStage.DECLINED, LinkStage.EXPIRED, LinkStage.FAILED), LinkStage.entries.filter { it.over }.toSet())
        assertEquals(2_000L, LINK_POLL_MS)
    }

    @Test
    fun eachStageOfTheNewDeviceSaysWhatIsHappening() {
        assertEquals("Open Lumia on a device that has your private messages. It will ask about this phone.", linkStageWords(LinkStage.WAITING))
        assertEquals("Check that your other device shows the same number, then approve it there.", linkStageWords(LinkStage.COMPARE))
        assertEquals("It wasn't approved on your other device.", linkStageWords(LinkStage.DECLINED))
        assertEquals("That took too long.", linkStageWords(LinkStage.EXPIRED))
        assertNull(linkStageWords(LinkStage.STARTING))
        assertNull(linkStageWords(LinkStage.FAILED))
        assertEquals("Too many devices are waiting to be linked. Wait a few minutes, then try again.", linkStartError("RATE_LIMITED", "slow down"))
        assertEquals("Linking didn't start: could not reach the instance", linkStartError(null, "could not reach the instance"))
        assertEquals("4 8 2 9 1 3", linkDigitsSpoken("482 913"))
    }

    // As the device that holds the identity: the prompt.

    @Test
    fun onePromptShowsAtATimeTheOldestFirstUntilItGoes() {
        assertNull(promptToShow(emptyList(), null))
        assertEquals("a", promptToShow(listOf("a", "b"), null))
        // The one showing stays while it is listed, even with an older one listed since.
        assertEquals("b", promptToShow(listOf("a", "b"), "b"))
        // Gone (declined, answered elsewhere, expired, linked), the oldest left is next.
        assertEquals("a", promptToShow(listOf("a", "c"), "b"))
        assertNull(promptToShow(emptyList(), "b"))
    }

    @Test
    fun aPromptsStageIsTheCoresAndOneUnknownIsNeverApproved() {
        assertEquals(PromptStage.NEW, promptStageOf("new"))
        assertEquals(PromptStage.WAITING, promptStageOf("waiting"))
        assertEquals(PromptStage.COMPARE, promptStageOf("compare"))
        assertEquals(PromptStage.REFUSED, promptStageOf("refused"))
        assertEquals(PromptStage.REFUSED, promptStageOf("something new"))
    }

    @Test
    fun aPromptNamesTheDeviceAsTheRequestDoes() {
        assertEquals("Pixel 9 wants your private messages", promptTitle(PromptStage.NEW, "native", " Pixel 9 ", ""))
        assertEquals("A browser on https://my.lumia.chat wants your private messages", promptTitle(PromptStage.NEW, "web", "Safari", "https://my.lumia.chat"))
        assertEquals("A browser wants your private messages", promptTitle(PromptStage.NEW, "web", "", ""))
        assertEquals("A new device wants your private messages", promptTitle(PromptStage.NEW, "native", "", ""))
        assertEquals("A bot wants your private messages", promptTitle(PromptStage.NEW, "bot", "", ""))
        assertEquals("Waiting for Pixel 9…", promptTitle(PromptStage.WAITING, "native", "Pixel 9", ""))
        assertEquals("Waiting for the browser on https://my.lumia.chat…", promptTitle(PromptStage.WAITING, "web", "Safari", "https://my.lumia.chat"))
        assertEquals("Does Pixel 9 show this number?", promptTitle(PromptStage.COMPARE, "native", "Pixel 9", ""))
        assertEquals("Does the new device show this number?", promptTitle(PromptStage.COMPARE, "native", "", ""))
        assertEquals("This request can't be trusted", promptTitle(PromptStage.REFUSED, "native", "Pixel 9", ""))
        assertEquals("Only continue if you're linking a device of yours right now.", promptLine(PromptStage.NEW))
        assertEquals("Don't approve it.", promptLine(PromptStage.REFUSED))
        assertNull(promptLine(PromptStage.WAITING))
        assertNull(promptLine(PromptStage.COMPARE))
        assertEquals("Pixel 9 is linked.", promptLinkedWords("native", "Pixel 9", ""))
        assertEquals("The browser on https://my.lumia.chat is linked.", promptLinkedWords("web", "", "https://my.lumia.chat"))
        assertEquals("Linking was stopped.", PROMPT_STOPPED)
    }

    @Test
    fun aPromptsFailedCallSaysWhyInWords() {
        assertEquals("Another of your devices answered it first.", promptError("FAILED_PRECONDITION", "answered"))
        assertTrue(promptError("NOT_FOUND", "not found").startsWith("That request is over."))
        assertTrue(promptError("IDENTITY_PROOF_INVALID", "bad roster").startsWith("The instance refused the new list of devices."))
        assertEquals("Too many tries. Wait a minute, then try again.", promptError("RATE_LIMITED", "slow down"))
        // The core's own words, a sentence.
        assertEquals("This request cannot be approved: start again on the new device", promptError(null, "this request cannot be approved: start again on the new device"))
    }

    // Restoring, keeping devices (identity spec 4.3 step 3).

    private fun device(id: String, kind: String = "native", label: String = "", origin: String = "", added: Long = 0, revoked: Boolean = false) =
        DeviceSummary(id, kind, label, origin, added, false, revoked)

    @Test
    fun theRestoreButtonSaysHowManyDevicesItKeeps() {
        assertEquals("Restore and stop them all", restoreKeepButton(0))
        assertEquals("Restore and keep 1 device", restoreKeepButton(1))
        assertEquals("Restore and keep 3 devices", restoreKeepButton(3))
    }

    @Test
    fun aDeviceToKeepReadsAsItsLabelAndWhenItWasAdded() {
        val utc = ZoneId.of("UTC")
        // 2026-11-12 15:00 UTC, in microseconds.
        val nov12 = 1_794_495_600_000_000L
        assertEquals("Added Nov 12", restoreDeviceMeta("", nov12, utc))
        assertEquals("On chat.acme.dev · added Nov 12", restoreDeviceMeta("https://chat.acme.dev", nov12, utc))
        assertEquals("On chat.acme.dev", restoreDeviceMeta("https://chat.acme.dev/", 0, utc))
        assertEquals("", restoreDeviceMeta("", 0, utc))
        assertEquals("MacBook Pro", restoreDeviceName("native", " MacBook Pro "))
        assertEquals("Browser", restoreDeviceName("web", ""))
        assertEquals("Device", restoreDeviceName("native", ""))
    }

    // An instance that knows no device of the identity restores at once, as before.
    @Test
    fun thePhraseLeadsToTheDevicesOnlyWhenThereAreAnyToStop() {
        assertTrue(restoreChoices(emptyList()).isEmpty())
        assertTrue(restoreChoices(listOf(device("a", revoked = true))).isEmpty())
        assertEquals(listOf("b"), restoreChoices(listOf(device("a", revoked = true), device("b"))).map { it.deviceId })
    }

    @Test
    fun aRestoreKeepsWhatIsTickedInTheListsOrderAndNeverSaysThePhrase() {
        val phrase = words.joinToString(" ")
        var r = Restoring(phrase, listOf(device("a"), device("b"), device("c")))
        assertEquals(emptyList<String>(), r.keep)
        r = r.toggled("c").toggled("a")
        assertEquals(listOf("a", "c"), r.keep)
        r = r.toggled("c")
        assertEquals(listOf("a"), r.keep)
        assertEquals(phrase, r.phrase)
        assertFalse(r.toString().contains("w1"))
    }
}
