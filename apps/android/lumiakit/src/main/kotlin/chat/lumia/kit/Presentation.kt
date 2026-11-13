// Presentation helpers shared by the layouts: initials, the avatar tone an
// id hashes to (the same nine as the web), times by the phone's clock.
package chat.lumia.kit

import android.content.Context
import android.text.format.DateFormat
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import java.time.Instant
import java.util.Locale
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

/**
 * The phone's clock: hours and minutes in 24 hours or 12 as its setting
 * says, which the language's own habit does not (a phone in English set to
 * 24 hours shows 14:05), written the language's way.
 */
fun clockOf(context: Context): DateTimeFormatter {
    val skeleton = if (DateFormat.is24HourFormat(context)) "Hm" else "hm"
    return DateTimeFormatter.ofPattern(DateFormat.getBestDateTimePattern(Locale.getDefault(), skeleton))
}

/** The phone's clock (`clockOf`), read again as the setting changes and kept while it does not. */
@Composable
fun rememberClock(): DateTimeFormatter {
    val context = LocalContext.current
    val hours24 = DateFormat.is24HourFormat(context)
    return remember(hours24, Locale.getDefault()) { clockOf(context) }
}

fun timeOfMicros(micros: Long, clock: DateTimeFormatter = timeFormat): String =
    Instant.ofEpochMilli(micros / 1000).atZone(ZoneId.systemDefault()).format(clock)

fun dayOfMicros(micros: Long): String =
    Instant.ofEpochMilli(micros / 1000).atZone(ZoneId.systemDefault()).format(dayFormat)

/** When a message was sent, as a row shows it: the time today, the day otherwise. */
fun rowTimeOfMicros(micros: Long, clock: DateTimeFormatter = timeFormat): String {
    val at = Instant.ofEpochMilli(micros / 1000).atZone(ZoneId.systemDefault())
    return if (at.toLocalDate() == LocalDate.now()) at.format(clock) else at.format(dayFormat)
}
