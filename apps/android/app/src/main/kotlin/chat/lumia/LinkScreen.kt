// Link this phone as a device of an identity. Signed in, by a prompt
// (identity spec 4.2.1): every device of the account that holds the
// identity is asked, the person checks that both show the same six digits
// and approves there, and nothing is copied or typed. "Link another way"
// is 4.2 by hand, for when the other device cannot reach the instance or
// this phone is not signed in: show the request for a device that holds
// the identity to approve, then claim the code it gives and sign in by
// identity proof.
package chat.lumia

import android.content.Intent
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import chat.lumia.kit.AppStore
import chat.lumia.kit.LINK_POLL_MS
import chat.lumia.kit.LINK_REQUEST_HOW
import chat.lumia.kit.LinkStage
import chat.lumia.kit.Linking
import chat.lumia.kit.LumiaCard
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.Route
import chat.lumia.kit.deviceLabel
import chat.lumia.kit.linkDigitsSpoken
import chat.lumia.kit.linkStageWords
import chat.lumia.kit.lumiaMessage
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun LinkScreen(store: AppStore, session: LumiaSession) {
    // By hand: chosen, or the only way for a phone not signed in, which has no session to open one with.
    // A phone that holds an identity already cannot open one either (only a debug launch brings it here).
    var another by rememberSaveable { mutableStateOf(!session.signedIn || session.hasIdentity) }
    val leave = {
        session.linkCancel()
        store.route = if (session.signedIn) Route.APP else Route.SIGN_IN
    }
    BackHandler(onBack = leave)

    Column(
        modifier = Modifier.fillMaxSize().systemBarsPadding().verticalScroll(rememberScrollState()).padding(LumiaTokens.space16).padding(top = LumiaTokens.space40),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
    ) {
        if (another) {
            LinkByHand(store, session, leave)
        } else {
            LinkByPrompt(store, session, leave) {
                // The other devices stop asking: this way, one of them approves a pasted request instead.
                session.linkCancel()
                another = true
            }
        }
    }
}

/**
 * The prompt's side on this phone: started as the screen opens, asked
 * again every [LINK_POLL_MS] while it is open (and at every account event,
 * by the session); waiting, then the digits, then linked, or declined or
 * expired with Try again.
 */
@Composable
private fun LinkByPrompt(store: AppStore, session: LumiaSession, leave: () -> Unit, byHand: () -> Unit) {
    val context = LocalContext.current
    val linking = session.linking
    val stage = linking?.stage
    LaunchedEffect(Unit) { if (session.linking == null) session.linkStart(deviceLabel) }
    LaunchedEffect(stage) {
        while (stage?.polls == true) {
            session.linkFollow()
            delay(LINK_POLL_MS)
        }
    }
    // Linked: this phone holds the identity and is signed in by it, as a claimed code leaves it.
    LaunchedEffect(stage) {
        if (stage != LinkStage.LINKED) return@LaunchedEffect
        session.linkDone()
        Toast.makeText(context, "Linked. Private messages are ready on this phone.", Toast.LENGTH_SHORT).show()
        store.route = Route.APP
    }

    LumiaCard("Link this phone", "A device you already use approves this one.") {
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            when {
                stage == null || stage == LinkStage.STARTING || stage == LinkStage.WAITING || stage == LinkStage.LINKED -> Waiting(linking)
                stage == LinkStage.COMPARE -> Digits(linking?.digits.orEmpty())
                else -> {
                    if (stage == LinkStage.FAILED) LumiaErrorText(linking?.error.orEmpty())
                    else Text(linkStageWords(stage).orEmpty(), style = LumiaTokens.textBody.style, color = LumiaTokens.inkBody.color)
                }
            }
            // A failed ask is said quietly and asked again; the stage on screen stays.
            linking?.error?.takeIf { stage?.polls == true }?.let { LumiaErrorText(it) }
            if (stage?.over == true) LumiaPrimaryButton("Try again", Modifier.fillMaxWidth()) { session.linkStart(deviceLabel) }
            LumiaSecondaryButton("Cancel", Modifier.fillMaxWidth(), enabled = stage != LinkStage.LINKED, onClick = leave)
            if (stage != LinkStage.COMPARE && stage != LinkStage.LINKED) {
                LumiaLinkButton("Link another way", Modifier.fillMaxWidth(), onClick = byHand)
            }
        }
    }
}

/** Waiting for another device to answer: the line, with a quiet spinner beside it. */
@Composable
private fun Waiting(linking: Linking?) {
    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12), verticalAlignment = Alignment.Top) {
        CircularProgressIndicator(Modifier.padding(top = LumiaTokens.space2).size(LumiaTokens.space18), color = LumiaTokens.inkMuted.color, strokeWidth = LumiaTokens.space2)
        Text(
            linkStageWords(LinkStage.WAITING).orEmpty(),
            style = LumiaTokens.textBody.style,
            color = LumiaTokens.inkBody.color,
            modifier = Modifier.weight(1f).semantics { liveRegion = LiveRegionMode.Polite },
        )
    }
    if (linking?.stage == LinkStage.LINKED) Text("Linked.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
}

/** The six digits, large and mono as the approve screen's code, and what to do with them. */
@Composable
private fun Digits(digits: String) {
    Column(
        Modifier.fillMaxWidth().padding(vertical = LumiaTokens.space12),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
    ) {
        Text(
            digits,
            style = LumiaTokens.titleAuth.style.copy(fontFamily = FontFamily.Monospace),
            color = LumiaTokens.ink.color,
            modifier = Modifier.semantics(mergeDescendants = true) {
                contentDescription = linkDigitsSpoken(digits)
                liveRegion = LiveRegionMode.Polite
            },
        )
    }
    Text(linkStageWords(LinkStage.COMPARE).orEmpty(), style = LumiaTokens.textBody.style, color = LumiaTokens.inkBody.color, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
}

/**
 * 4.2 by hand: the request to copy or share to a device that holds the
 * identity, then the code it gives typed here, which claims and signs in
 * by identity proof.
 */
@Composable
private fun LinkByHand(store: AppStore, session: LumiaSession, leave: () -> Unit) {
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current
    var request by rememberSaveable { mutableStateOf("") }
    var code by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var copied by rememberSaveable { mutableStateOf(false) }

    LumiaCard("Link this phone", "A device you already use approves this one.") {
        Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
            if (request.isEmpty()) {
                Text("This phone gets its own device key.", style = LumiaTokens.textBody.style, color = LumiaTokens.inkSecondary.color)
                LumiaPrimaryButton("Start linking", modifier = Modifier.fillMaxWidth()) {
                    scope.launch {
                        error = ""
                        try { request = session.identityLinkRequest(deviceLabel) } catch (e: Exception) { error = e.lumiaMessage }
                    }
                }
            } else {
                Text(LINK_REQUEST_HOW, style = LumiaTokens.textBody.style, color = LumiaTokens.inkSecondary.color)
                SelectionContainer {
                    Text(
                        request,
                        style = LumiaTokens.textMeta.style.copy(fontFamily = FontFamily.Monospace),
                        color = LumiaTokens.inkField.color,
                        modifier = Modifier.fillMaxWidth().background(LumiaTokens.field.color, RoundedCornerShape(LumiaTokens.radius8)).padding(LumiaTokens.space10),
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                    LumiaSecondaryButton(if (copied) "Copied" else "Copy request") {
                        clipboard.setText(AnnotatedString(request))
                        copied = true
                    }
                    LumiaSecondaryButton("Share") {
                        val send = Intent(Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(Intent.EXTRA_TEXT, request) }
                        context.startActivity(Intent.createChooser(send, "Share the link request"))
                    }
                }
                LumiaField("Code from the approving device", code, { code = it }, placeholder = "8 characters")
                LumiaPrimaryButton("Claim and sign in", enabled = !busy && code.isNotBlank(), modifier = Modifier.fillMaxWidth()) {
                    scope.launch {
                        error = ""
                        busy = true
                        try {
                            // In the store's scope: a code the instance has taken is not left half-claimed by the phone being turned.
                            val claimed = code.trim()
                            store.signIn { identityClaim(claimed) }
                        } catch (e: Exception) {
                            error = e.lumiaMessage
                        } finally {
                            busy = false
                        }
                    }
                }
            }
            if (error.isNotEmpty()) LumiaErrorText(error)
            if (!session.signedIn) {
                Text("Signed in first, your other device simply asks you, with nothing to copy.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
            }
            LumiaLinkButton(if (session.signedIn) "Cancel" else "Back to sign in", modifier = Modifier.fillMaxWidth(), onClick = leave)
        }
    }
}
