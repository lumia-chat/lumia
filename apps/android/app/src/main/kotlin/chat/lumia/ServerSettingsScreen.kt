// A server's settings (architecture 6.7): its picture and name, and how
// long it keeps its messages. Offered from the server's channel list to
// whoever may change them; the web's and the desktop's screen, in the
// vocabulary of Profile: a card for each, a Save for each. Each change
// answers with the snapshot as it then is, so the rail and the header show
// it at once; other members hear of it through their account logs.
package chat.lumia

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import chat.lumia.kit.LumiaCard
import chat.lumia.kit.LumiaChoice
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaPicture
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.PICTURE_MAX_BYTES
import chat.lumia.kit.PICTURE_TOO_LARGE
import chat.lumia.kit.Retention
import chat.lumia.kit.SERVER_NAME_REFUSED
import chat.lumia.kit.initialsOf
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaDetails
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.pictureRefused
import chat.lumia.kit.readAtMost
import chat.lumia.kit.serverNameChanged
import chat.lumia.kit.serverNameFits
import chat.lumia.kit.serverSettingsRefused
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.lumia_ffi.ServerSummary

/** The picture in Overview: a rail tile's shape, larger (`tokens.json` has no name for it). */
private val PictureSize = LumiaTokens.tileSize + LumiaTokens.space12

@Composable
fun ServerSettingsScreen(session: LumiaSession, server: ServerSummary?, back: () -> Unit) {
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    val scope = rememberCoroutineScope()
    val serverId = server?.serverId ?: ""
    val name = server?.name ?: ""
    val kept = server?.retentionDays?.toInt() ?: Retention.FOREVER

    var draft by rememberSaveable(serverId) { mutableStateOf(name) }
    var nameBusy by remember { mutableStateOf(false) }
    var nameError by rememberSaveable { mutableStateOf("") }
    var nameSaved by rememberSaveable { mutableStateOf(false) }

    var pictureBusy by remember { mutableStateOf(false) }
    var pictureError by rememberSaveable { mutableStateOf("") }

    var retention by rememberSaveable(serverId) { mutableStateOf(kept) }
    var retentionBusy by remember { mutableStateOf(false) }
    var retentionError by rememberSaveable { mutableStateOf("") }
    var retentionSaved by rememberSaveable { mutableStateOf(false) }

    fun pictureFailed(e: Exception) {
        pictureError = pictureRefused(e.lumiaCode, e.lumiaDetails["reason"]) ?: e.lumiaMessage
    }

    // The system's photo picker: nothing of the phone's pictures is asked
    // for but the one chosen. The instance makes its own square of it.
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri == null || serverId.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            pictureBusy = true
            pictureError = ""
            try {
                val (bytes, type) = withContext(Dispatchers.IO) {
                    val resolver = context.contentResolver
                    val bytes = resolver.openInputStream(uri)?.use { readAtMost(it, PICTURE_MAX_BYTES) }
                    bytes to (resolver.getType(uri) ?: "application/octet-stream")
                }
                // What the instance would refuse for its size is not sent at all.
                if (bytes == null) pictureError = PICTURE_TOO_LARGE else session.setServerIcon(serverId, bytes, type)
            } catch (e: Exception) {
                pictureFailed(e)
            } finally {
                pictureBusy = false
            }
        }
    }

    fun removePicture() {
        scope.launch {
            pictureBusy = true
            pictureError = ""
            try {
                session.removeServerIcon(serverId)
            } catch (e: Exception) {
                pictureFailed(e)
            } finally {
                pictureBusy = false
            }
        }
    }

    fun saveName() {
        val next = draft.trim()
        nameSaved = false
        if (!serverNameFits(next)) {
            nameError = SERVER_NAME_REFUSED
            return
        }
        scope.launch {
            nameBusy = true
            nameError = ""
            try {
                session.updateServer(serverId, next, null)
                draft = next
                nameSaved = true
                // The keyboard goes with the edit, and the rest of the screen is there again.
                focus.clearFocus()
            } catch (e: Exception) {
                nameError = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
            } finally {
                nameBusy = false
            }
        }
    }

    fun saveRetention() {
        val days = retention
        scope.launch {
            retentionBusy = true
            retentionError = ""
            retentionSaved = false
            try {
                session.updateServer(serverId, null, days)
                retentionSaved = true
            } catch (e: Exception) {
                retentionError = serverSettingsRefused(e.lumiaCode, e.lumiaDetails["field"]) ?: e.lumiaMessage
            } finally {
                retentionBusy = false
            }
        }
    }

    val picture = server?.let { session.pictureOf(it.iconUrl) }
    PanelScreen(back = name.ifEmpty { "Server" }, onBack = back, title = "Server settings", subtitle = name) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).navigationBarsPadding().imePadding().padding(LumiaTokens.space16), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            LumiaCard("Overview") {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12), verticalAlignment = Alignment.CenterVertically) {
                        // The picture as the rail draws the server, over its initials.
                        Box(
                            Modifier
                                .size(PictureSize)
                                .clip(RoundedCornerShape(LumiaTokens.radius14))
                                // The rail's tile colour is the card's own; the choice's fill sets it apart.
                                .background(LumiaTokens.chip.color)
                                .border(1.dp, LumiaTokens.line.color, RoundedCornerShape(LumiaTokens.radius14))
                                .clearAndSetSemantics { contentDescription = if (picture != null) "$name's picture" else "$name has no picture" },
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(initialsOf(name), style = LumiaTokens.titleMonogram.style, color = LumiaTokens.ink.color)
                            LumiaPicture(picture, Modifier.matchParentSize())
                        }
                        Column(horizontalAlignment = Alignment.Start) {
                            LumiaSecondaryButton("Choose a picture…", enabled = !pictureBusy && server != null) {
                                picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                            }
                            if (picture != null) LumiaLinkButton("Remove", enabled = !pictureBusy) { removePicture() }
                        }
                    }
                    if (pictureError.isNotEmpty()) LumiaErrorText(pictureError)
                    LumiaField("Name", draft, { draft = it; nameSaved = false; nameError = "" }, placeholder = "Our place")
                    if (nameError.isNotEmpty()) LumiaErrorText(nameError)
                    if (nameSaved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                    LumiaPrimaryButton("Save", Modifier.fillMaxWidth(), enabled = !nameBusy && server != null && serverNameChanged(draft, name)) { saveName() }
                }
            }
            LumiaCard("Message history", "How long this server keeps its messages.") {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    LumiaChoice(
                        Retention.choices(kept).map { it.toString() to Retention.label(it) },
                        retention.toString(),
                        { retention = it.toInt(); retentionSaved = false; retentionError = "" },
                    )
                    // Nothing is deleted until Save; this says what Save would delete.
                    Retention.warning(kept, retention)?.let { LumiaErrorText(it) }
                    if (retentionError.isNotEmpty()) LumiaErrorText(retentionError)
                    if (retentionSaved) Text("Saved.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                    LumiaPrimaryButton("Save", Modifier.fillMaxWidth(), enabled = !retentionBusy && server != null && retention != kept) { saveRetention() }
                }
            }
            // The channels every newcomer joins (architecture 6.8).
            if (server != null) DefaultChannelsCard(session, server)
        }
    }
}
