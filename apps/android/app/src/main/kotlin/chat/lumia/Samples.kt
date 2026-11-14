// What a debug launch with LUMIA_DEV_SCREEN=bodies draws, with no instance
// and no session: message rows over bodies that show what the phone does
// with a long message, a code block, a quote, a list and the Markdown in a
// paragraph, and the composer's counter near and over the limit, so a
// change to how a body is drawn can be looked at in both themes
// (LUMIA_DEV_APPEARANCE) without seeding anything.
package chat.lumia

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.TextFieldValue
import chat.lumia.kit.AvatarTone
import chat.lumia.kit.BlockPlace
import chat.lumia.kit.LumiaComposer
import chat.lumia.kit.LumiaMessageRow
import chat.lumia.kit.LumiaTokens
import uniffi.lumia_ffi.MentionSummary
import uniffi.lumia_ffi.MessageRoom

private class Sample(val id: String, val author: String, val body: String, val mentions: List<MentionSummary> = emptyList(), val edited: Boolean = false, val washed: Boolean = false)

private val samples = listOf(
    Sample(
        "md", "Ada Lovelace",
        "Shipped *tonight*, **not** ~~tomorrow~~: see `cargo test` and [the notes](https://lumia.chat). Ping @grace if it breaks 😀 **again**.",
        mentions = listOf(MentionSummary("grace", false, 100u, 106u)),
        edited = true,
    ),
    Sample(
        "code", "Grace Hopper",
        "The fix:\n\n```rust\nfn main() {\n    let message = \"a line long enough that it does not fit on a phone and has to scroll sideways inside its block\";\n    println!(\"{message}\");\n}\n```",
        edited = true,
    ),
    Sample("quote", "Ada Lovelace", "> The Analytical Engine weaves algebraic patterns\n> just as the Jacquard loom weaves flowers.\n\nStill true.\n\n1. One\n2. Two, with a nested list:\n   - inner\n   - another\n3. Three"),
    Sample("long", "Grace Hopper", (1..40).joinToString("\n") { "Line $it of a long paste that goes on for a while." }, washed = true),
    Sample("word", "Ada Lovelace", "A long word: " + "supercalifragilistic".repeat(6)),
)

@Composable
fun BodySamples() {
    val expanded = remember { mutableStateMapOf<String, Boolean>() }
    var draft by remember { mutableStateOf(TextFieldValue("A draft near the limit")) }
    PanelScreen(back = "Home", onBack = {}, title = "# bodies") {
        LazyColumn(
            Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(vertical = LumiaTokens.space8),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4),
        ) {
            itemsIndexed(samples, key = { _, s -> s.id }) { _, s ->
                LumiaMessageRow(
                    s.author, AvatarTone.of(s.author), "12:04", s.body, place = BlockPlace.Alone,
                    mentions = s.mentions, nameOf = { if (it == "grace") "Grace Hopper" else null },
                    edited = s.edited, washed = s.washed,
                    expanded = expanded[s.id] == true, onExpanded = { expanded[s.id] = it },
                )
            }
            item { LumiaComposer(draft, { draft = it }, "Message #general", room = MessageRoom(true, 1204)) {} }
            item { LumiaComposer(draft, { draft = it }, "Message #general", room = MessageRoom(true, -1)) {} }
        }
    }
}
