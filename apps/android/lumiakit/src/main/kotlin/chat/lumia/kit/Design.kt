// The types the generated tokens are expressed in, and how they become
// Compose values. Colours resolve per theme, which is the one Preferences'
// appearance names (`LumiaTheme`) and the system's where nothing names
// one; type styles map onto the system font until the brand faces ship as
// TrueType.
package chat.lumia.kit

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit

/**
 * Whether what is under a [LumiaTheme] is drawn dark; null outside one,
 * where the colours are the system's theme.
 */
val LocalLumiaDark = staticCompositionLocalOf<Boolean?> { null }

/** A colour with a value per theme. */
data class LumiaColor(val light: Color, val dark: Color) {
    /** The colour for the current theme. */
    val color: Color
        @Composable get() = if (LocalLumiaDark.current ?: isSystemInDarkTheme()) dark else light
}

/**
 * The app in one theme, every screen of it: light or dark as Preferences'
 * appearance resolves (`Appearance.dark`), whatever the phone is in. Text
 * that sets no colour of its own is in the theme's ink.
 */
@Composable
fun LumiaTheme(dark: Boolean, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalLumiaDark provides dark) {
        CompositionLocalProvider(LocalContentColor provides LumiaTokens.ink.color, content = content)
    }
}

/** A type style: size, CSS weight, display face or text face. */
data class LumiaType(val size: TextUnit, val weight: Int, val display: Boolean) {
    val style: TextStyle
        get() = TextStyle(
            fontSize = size,
            fontWeight = FontWeight(weight),
            fontFamily = if (display) FontFamily.SansSerif else FontFamily.Default,
        )
}
