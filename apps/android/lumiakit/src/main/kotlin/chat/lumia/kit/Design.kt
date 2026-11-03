// The types the generated tokens are expressed in, and how they become
// Compose values. Colours resolve per theme; type styles map onto the
// system font until the brand faces ship as TrueType.
package chat.lumia.kit

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit

/** A colour with a value per theme. */
data class LumiaColor(val light: Color, val dark: Color) {
    /** The colour for the current theme. */
    val color: Color
        @Composable get() = if (isSystemInDarkTheme()) dark else light
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
