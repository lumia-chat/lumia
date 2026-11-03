// Presentation helpers shared by the layouts: initials, the avatar tone an
// id hashes to (the same nine as the web), times.
package chat.lumia.kit

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

enum class AvatarTone {
    PEACH, BLUE, PURPLE, MINT, AMBER, SKY, STONE, GREEN, INDIGO;

    val fill: LumiaColor
        get() = when (this) {
            PEACH -> LumiaTokens.tonePeach
            BLUE -> LumiaTokens.toneBlue
            PURPLE -> LumiaTokens.tonePurple
            MINT -> LumiaTokens.toneMint
            AMBER -> LumiaTokens.toneAmber
            SKY -> LumiaTokens.toneSky
            STONE -> LumiaTokens.toneStone
            GREEN -> LumiaTokens.toneGreen
            INDIGO -> LumiaTokens.toneIndigo
        }

    val ink: LumiaColor
        get() = when (this) {
            PEACH -> LumiaTokens.tonePeachInk
            BLUE -> LumiaTokens.toneBlueInk
            PURPLE -> LumiaTokens.tonePurpleInk
            MINT -> LumiaTokens.toneMintInk
            AMBER -> LumiaTokens.toneAmberInk
            SKY -> LumiaTokens.toneSkyInk
            STONE -> LumiaTokens.toneStoneInk
            GREEN -> LumiaTokens.toneGreenInk
            INDIGO -> LumiaTokens.toneIndigoInk
        }

    companion object {
        /** The web's `toneFor`: a stable hash of the id over the nine tones. */
        fun of(id: String): AvatarTone {
            var h = 0u
            for (c in id) h = h * 31u + c.code.toUInt()
            return entries[(h % entries.size.toUInt()).toInt()]
        }
    }
}

fun initialsOf(name: String): String {
    val s = name.split(Regex("\\s+")).filter { it.isNotEmpty() }.take(2)
        .mapNotNull { it.firstOrNull()?.uppercase() }.joinToString("")
    return s.ifEmpty { "?" }
}

private val timeFormat: DateTimeFormatter = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT)
private val dayFormat: DateTimeFormatter = DateTimeFormatter.ofPattern("MMM d")

fun timeOfMicros(micros: Long): String =
    Instant.ofEpochMilli(micros / 1000).atZone(ZoneId.systemDefault()).format(timeFormat)

fun dayOfMicros(micros: Long): String =
    Instant.ofEpochMilli(micros / 1000).atZone(ZoneId.systemDefault()).format(dayFormat)

/** When a message was sent, as a row shows it: the time today, the day otherwise. */
fun rowTimeOfMicros(micros: Long): String {
    val at = Instant.ofEpochMilli(micros / 1000).atZone(ZoneId.systemDefault())
    return if (at.toLocalDate() == LocalDate.now()) at.format(timeFormat) else at.format(dayFormat)
}
