// The rules a conversation draws by: which messages are one block, who is
// typing and how that reads. The desktop shell's tests of the same rules
// are `messages_one_person_sends_close_together_are_one_block` and the
// typing tests in apps/desktop/tests/shell.rs. And which failures mean the
// session is over, the desktop's `a_revoked_session_goes_back_to_sign_in_and_says_why`.
package chat.lumia.kit

import java.time.LocalDateTime
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.lumia_ffi.LumiaException

class TimelineTest {
    private val zone = ZoneId.of("America/Halifax")
    private val minute = 60_000_000L
    private val day = 24 * 60 * minute

    private fun at(hour: Int, minute: Int): Long =
        LocalDateTime.of(2026, 11, 12, hour, minute).atZone(zone).toInstant().toEpochMilli() * 1000

    private val noon = at(12, 0)

    @Test
    fun messagesOnePersonSendsCloseTogetherAreOneBlock() {
        val stamps = listOf(
            Stamp("grace", noon),
            Stamp("grace", noon + minute),
            // More than five minutes after the one before it: a block of its own.
            Stamp("grace", noon + 7 * minute),
            Stamp("me", noon + 8 * minute),
            Stamp("me", noon + 9 * minute),
            // The same person a day on: a day's first message is under its own name.
            Stamp("me", noon + 9 * minute + day),
            Stamp("me", noon + 10 * minute + day),
        )
        val places = blockPlaces(stamps, zone)
        assertEquals("who is named: the first message of each block", listOf(true, false, true, true, false, true, false), places.map { it.opens })
        assertEquals("where a block ends", listOf(false, true, true, false, true, false, true), places.map { it.closes })
    }

    @Test
    fun aBlockIsMeasuredFromTheMessageBeforeNotFromItsFirst() {
        // Four minutes apart each time: twelve minutes from the first to the last, and one block.
        val stamps = (0..3).map { Stamp("grace", noon + it * 4 * minute) }
        assertEquals(listOf(true, false, false, false), blockPlaces(stamps, zone).map { it.opens })
    }

    @Test
    fun fiveMinutesIsTooLongAndTheOrderOfTheClocksDoesNotMatter() {
        val before = Stamp("grace", noon)
        assertTrue(joins(before, Stamp("grace", noon + 5 * minute - 1), zone))
        assertFalse(joins(before, Stamp("grace", noon + 5 * minute), zone))
        // A message stamped by another clock, a little before the one it follows.
        assertTrue(joins(before, Stamp("grace", noon - minute), zone))
        assertFalse(joins(before, Stamp("grace", noon - 5 * minute), zone))
    }

    @Test
    fun anotherPersonOrNoKnownPersonStartsABlock() {
        assertFalse(joins(Stamp("grace", noon), Stamp("alan", noon + minute), zone))
        assertFalse("a message with no author is no one's", joins(Stamp("", noon), Stamp("", noon + minute), zone))
    }

    @Test
    fun aDayIsTheReadersDay() {
        // Two minutes apart across midnight in Halifax: two days there, one in Vancouver.
        val late = Stamp("grace", at(23, 59))
        val early = Stamp("grace", at(23, 59) + 2 * minute)
        assertFalse(joins(late, early, zone))
        assertTrue(joins(late, early, ZoneId.of("America/Vancouver")))
    }

    @Test
    fun oneMessageAndNoneHaveTheirPlaces() {
        assertEquals(emptyList<BlockPlace>(), blockPlaces(emptyList(), zone))
        assertEquals(listOf(BlockPlace.Alone), blockPlaces(listOf(Stamp("grace", noon)), zone))
    }

    @Test
    fun thoseTypingAreNamedUpToTwo() {
        assertEquals(TypingWords("", 0), typingWords(emptyList()))
        assertEquals(TypingWords("Grace Hopper is typing…", "Grace Hopper".length), typingWords(listOf("Grace Hopper")))
        assertEquals(
            TypingWords("Grace Hopper and Alan Turing are typing…", "Grace Hopper and Alan Turing".length),
            typingWords(listOf("Grace Hopper", "Alan Turing")),
        )
        // "Several people" is no one's name: none of it is drawn as one.
        assertEquals(TypingWords("Several people are typing…", 0), typingWords(listOf("Grace Hopper", "Alan Turing", "Ada Lovelace")))
    }

    @Test
    fun thoseTypingAreKeptByChannelInTheOrderTheyBegan() {
        val typing = Typing()
            .began("general", "grace", 8_000_000)
            .began("general", "alan", 9_000_000)
            .began("random", "ada", 5_000_000)
        assertEquals(listOf("grace", "alan"), typing.inChannel("general"))
        assertEquals(listOf("ada"), typing.inChannel("random"))
        assertEquals(emptyList<String>(), typing.inChannel("elsewhere"))
        assertEquals(5_000_000L, typing.ends)

        // Again from someone already typing only moves their end: grace keeps her place.
        val again = typing.began("general", "grace", 12_000_000)
        assertEquals(listOf("grace", "alan"), again.inChannel("general"))
        assertEquals(again, again.began("general", "grace", 12_000_000))
        assertEquals(Typing().began("general", "alan", 9_000_000), again.expired(9_000_000 - 1).stopped("general", "grace").stopped("random", "ada"))
    }

    @Test
    fun aMessageEndsItsAuthorsTypingInThatChannelOnly() {
        val typing = Typing().began("general", "grace", 8_000_000).began("random", "grace", 8_000_000).began("general", "alan", 8_000_000)
        val after = typing.stopped("general", "grace")
        assertEquals(listOf("alan"), after.inChannel("general"))
        assertEquals(listOf("grace"), after.inChannel("random"))
        // From someone who was not typing, nothing changes: the same value, so nothing is redrawn.
        assertSame(after, after.stopped("general", "ada"))
    }

    @Test
    fun silenceEndsTypingWhenItsTimeRunsOut() {
        val typing = Typing().began("general", "grace", 8_000_000).began("general", "alan", 9_000_000)
        assertSame(typing, typing.expired(7_999_999))
        // At its end and not after it: `until` is when it stops being shown.
        val one = typing.expired(8_000_000)
        assertEquals(listOf("alan"), one.inChannel("general"))
        assertEquals(9_000_000L, one.ends)
        val none = one.expired(9_000_000)
        assertEquals(emptyList<String>(), none.inChannel("general"))
        assertNull(none.ends)
        assertEquals(Typing(), none)
    }

    // A session the instance revoked answers `SESSION_REVOKED`, not `UNAUTHENTICATED`,
    // a revoked device `DEVICE_REVOKED`, a refresh token run out `TOKEN_EXPIRED`. Taking
    // only the first for the end left the person in the shell with nothing that worked.
    @Test
    fun fourOfTheInstancesRefusalsMeanTheSessionIsOver() {
        for (code in listOf("UNAUTHENTICATED", "TOKEN_EXPIRED", "SESSION_REVOKED", "DEVICE_REVOKED")) {
            assertTrue(code, sessionOver(code))
            assertTrue(code, LumiaException.Api(code, "session revoked", emptyMap()).endsSession)
        }
    }

    @Test
    fun noOtherRefusalEndsTheSession() {
        // The fifth 401 of protocol 10 is a proof refused, not a session; the rest are a call that failed.
        for (code in listOf("IDENTITY_PROOF_INVALID", "PERMISSION_DENIED", "NOT_FOUND", "FAILED_PRECONDITION", "RATE_LIMITED", "UNAVAILABLE", "INTERNAL", "session_revoked", "")) {
            assertFalse(code, sessionOver(code))
            assertFalse(code, LumiaException.Api(code, "no", emptyMap()).endsSession)
        }
        assertFalse(sessionOver(null))
        // No answer at all (no network) is no refusal.
        assertFalse(LumiaException.Other("error sending request").endsSession)
        assertFalse(IllegalStateException("anything else").endsSession)
    }

    @Test
    fun theCoreSayingSoItselfEndsTheSession() {
        // No tokens left to send, and this device revoked in the roster it holds.
        assertTrue(LumiaException.NotSignedIn().endsSession)
        assertTrue(LumiaException.DeviceRevoked(4uL).endsSession)
    }
}
