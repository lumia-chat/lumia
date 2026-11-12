// Link this phone as a device of an identity (identity spec 4.2): show
// the request for a device that holds the identity to approve, then
// claim the code it gives and sign in by identity proof.
package chat.lumia

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import chat.lumia.kit.AppStore
import chat.lumia.kit.LumiaCard
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.Route
import chat.lumia.kit.lumiaMessage
import kotlinx.coroutines.launch

@Composable
fun LinkScreen(store: AppStore, session: LumiaSession) {
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current
    var request by rememberSaveable { mutableStateOf("") }
    var code by rememberSaveable { mutableStateOf("") }
    var error by rememberSaveable { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var copied by rememberSaveable { mutableStateOf(false) }

    Column(
        modifier = Modifier.fillMaxSize().systemBarsPadding().verticalScroll(rememberScrollState()).padding(LumiaTokens.space16).padding(top = LumiaTokens.space40),
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
    ) {
        LumiaCard("Link this phone", "A device you already use approves this one.") {
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                if (request.isEmpty()) {
                    Text("This phone gets its own device key.", style = LumiaTokens.textBody.style, color = LumiaTokens.inkSecondary.color)
                    LumiaPrimaryButton("Start linking", modifier = Modifier.fillMaxWidth()) {
                        scope.launch {
                            error = ""
                            try { request = session.identityLinkRequest("phone") } catch (e: Exception) { error = e.lumiaMessage }
                        }
                    }
                } else {
                    Text("On a device that holds your identity, approve this request and enter the code it gives you.", style = LumiaTokens.textBody.style, color = LumiaTokens.inkSecondary.color)
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
                LumiaLinkButton("Back to sign in", modifier = Modifier.fillMaxWidth()) { store.route = if (session.signedIn) Route.APP else Route.SIGN_IN }
            }
        }
    }
}
