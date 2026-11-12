// Sign in on a phone: the instance (with its trust card, architecture
// 6.1), then a password with a second factor when the account asks for
// one, or a new account, or linking this phone from a device that holds
// the identity; the same flow as the web's, the macOS and the iOS apps'.
package chat.lumia

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AppStore
import chat.lumia.kit.LumiaCard
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Email
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaNotice
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.Route
import chat.lumia.kit.initialsOf
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaDetails
import chat.lumia.kit.lumiaMessage
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.InstanceSummary

@Composable
fun SignInScreen(store: AppStore) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var instance by rememberSaveable { mutableStateOf(store.savedInstance ?: "") }
    var info by rememberSaveable { mutableStateOf<InstanceSummary?>(null) }
    var reaching by rememberSaveable { mutableStateOf(false) }
    var creating by rememberSaveable { mutableStateOf(false) }
    var handle by rememberSaveable { mutableStateOf("") }
    var password by rememberSaveable { mutableStateOf("") }
    var totp by rememberSaveable { mutableStateOf("") }
    var needsTotp by rememberSaveable { mutableStateOf(false) }
    var error by rememberSaveable { mutableStateOf("") }
    var magicSent by rememberSaveable { mutableStateOf(false) }
    var magicCode by rememberSaveable { mutableStateOf("") }
    var busy by rememberSaveable { mutableStateOf(false) }

    suspend fun reach() {
        val session = store.session ?: return
        reaching = true
        try {
            info = session.instanceInfo()
        } catch (e: Exception) {
            error = "Could not reach that instance: ${e.lumiaMessage}"
        } finally {
            reaching = false
        }
    }

    suspend fun connect() {
        error = ""
        var url = instance.trim()
        while (url.endsWith("/")) url = url.dropLast(1)
        if (!(url.startsWith("http://") || url.startsWith("https://"))) {
            error = "Enter the instance address, starting with http:// or https://"
            return
        }
        try {
            store.open(url)
            reach()
        } catch (e: Exception) {
            error = e.lumiaMessage
        }
    }

    suspend fun requestMagicLink() {
        val session = store.session ?: return
        error = ""
        busy = true
        try {
            session.requestMagicLink(handle)
            magicSent = true
        } catch (e: Exception) {
            error = if (e.lumiaCode == "RATE_LIMITED") "Too many links in the last hour; try later." else e.lumiaMessage
        } finally {
            busy = false
        }
    }

    suspend fun completeMagicLink() {
        val session = store.session ?: return
        error = ""
        busy = true
        try {
            session.signInMagicLink(magicCode)
            store.route = Route.APP
        } catch (e: Exception) {
            error = if (e.lumiaCode == "UNAUTHENTICATED") "That code is unknown, used or expired. Ask for a new link." else e.lumiaMessage
        } finally {
            busy = false
        }
    }

    suspend fun browserSignIn(oidc: Boolean) {
        val session = store.session ?: return
        error = ""
        try {
            openInBrowser(context, session.browserSignInUrl(oidc))
        } catch (e: Exception) {
            error = e.lumiaMessage
        }
    }

    // Back from the browser with a one-time token on the app's own scheme.
    LaunchedEffect(store.browserReturn) {
        val back = store.browserReturn ?: return@LaunchedEffect
        store.browserReturn = null
        val session = store.session ?: return@LaunchedEffect
        busy = true
        try {
            session.completeBrowserSignIn(back)
            store.route = Route.APP
        } catch (e: Exception) {
            error = if (e.lumiaCode == "UNAUTHENTICATED") "That sign-in was not accepted." else e.lumiaMessage
        } finally {
            busy = false
        }
    }

    suspend fun submit() {
        val session = store.session ?: return
        error = ""
        busy = true
        try {
            if (creating) session.createAccount(handle, password)
            else session.signIn(handle, password, if (needsTotp) totp else null)
            store.route = Route.APP
        } catch (e: Exception) {
            when {
                // The passkey factor needs the browser's ceremony (architecture 6.3): the web sign-in does it.
                e.lumiaCode == "FAILED_PRECONDITION" && e.lumiaDetails["mfa"] == "passkey" -> error = "This account asks for its passkey at sign-in; use \"Sign in through the browser\"."
                e.lumiaCode == "FAILED_PRECONDITION" && !needsTotp -> needsTotp = true
                e.lumiaCode == "UNAUTHENTICATED" -> error = if (needsTotp) "That code was not accepted." else "Wrong handle or password."
                e.lumiaCode == "ALREADY_EXISTS" -> error = "That handle is taken."
                else -> error = e.lumiaMessage
            }
        } finally {
            busy = false
        }
    }

    LaunchedEffect(Unit) {
        // Sent back here by the instance (a session revoked or run out), the screen opens saying so.
        store.session?.takeEndedNotice()?.let { error = it }
        if (store.session != null && info == null) reach()
    }

    Column(
        modifier = Modifier.fillMaxSize().systemBarsPadding().verticalScroll(rememberScrollState()).padding(LumiaTokens.space16),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space16),
    ) {
        Image(painterResource(R.drawable.logo), contentDescription = "Lumia", modifier = Modifier.padding(top = LumiaTokens.space24).size(96.dp))
        val current = info
        if (current == null) {
            LumiaCard("Welcome", "Which instance do you want to sign in to?") {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    LumiaField("Instance", instance, { instance = it }, placeholder = "https://chat.example.com", keyboard = KeyboardOptions(keyboardType = KeyboardType.Uri))
                    if (error.isNotEmpty()) LumiaErrorText(error)
                    val openError = store.openError
                    if (openError != null && error.isEmpty()) LumiaErrorText(openError)
                    if (reaching) Text("Reaching the instance…", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                    LumiaPrimaryButton("Continue", enabled = !reaching, modifier = Modifier.fillMaxWidth()) { scope.launch { connect() } }
                }
            }
        } else {
            LumiaCard(
                if (creating) "Create an account" else "Welcome back",
                if (creating) "A new account on ${current.host}." else "Sign in to pick up your conversations.",
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12)) {
                    // The trust card as a row: the instance's tile, host, operator and version.
                    Row(
                        modifier = Modifier.fillMaxWidth().background(LumiaTokens.inset.color, RoundedCornerShape(LumiaTokens.radius12)).padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space6),
                        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(Modifier.size(34.dp).background(LumiaTokens.tile.color, CircleShape), contentAlignment = Alignment.Center) {
                            Text(initialsOf(current.operatorName.ifEmpty { current.host }).take(1), style = LumiaTokens.titleMonogram.style, color = LumiaTokens.ink.color)
                        }
                        Column(Modifier.weight(1f)) {
                            Text(current.host, style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkEmphasis.color, maxLines = 1)
                            Text(
                                "Run by ${current.operatorName.ifEmpty { "an unnamed operator" }}${if (current.softwareVersion.isEmpty()) "" else " · Lumia ${current.softwareVersion}"}",
                                style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, maxLines = 1,
                            )
                        }
                        LumiaLinkButton("Change") { info = null; error = "" }
                    }
                    LumiaField(if (creating) "Handle" else "Email or handle", handle, { handle = it }, placeholder = if (creating) "yourname" else "you@example.com", keyboard = KeyboardOptions(keyboardType = KeyboardType.Email))
                    LumiaField("Password", password, { password = it }, placeholder = if (creating) "At least 12 characters" else "Your password", secure = true)
                    if (needsTotp) LumiaField("Verification code", totp, { totp = it }, placeholder = "6 digits from your authenticator", keyboard = KeyboardOptions(keyboardType = KeyboardType.Number))
                    if (error.isNotEmpty()) LumiaErrorText(error)
                    LumiaPrimaryButton(if (creating) "Create account" else "Sign in", enabled = !busy, modifier = Modifier.fillMaxWidth()) { scope.launch { submit() } }
                    // Through the system browser (architecture 6.3): the provider,
                    // or the instance's web sign-in page for a passkey; the tab
                    // comes back with a one-time token, taken below.
                    if (!creating && current.oidcName.isNotEmpty()) {
                        LumiaSecondaryButton(current.oidcName, enabled = !busy, modifier = Modifier.fillMaxWidth()) { scope.launch { browserSignIn(oidc = true) } }
                    }
                    if (!creating) {
                        LumiaSecondaryButton("Sign in with a passkey", enabled = !busy, modifier = Modifier.fillMaxWidth()) { scope.launch { browserSignIn(oidc = false) } }
                    }
                    if (!creating && current.magicLinkEnabled) {
                        if (magicSent) {
                            LumiaNotice(Icons.Filled.Email, text = "A link is on its way to ${handle.trim()} if it has an account here. Open it on this device, or paste its code below.")
                            LumiaField("Code from the email", magicCode, { magicCode = it }, placeholder = "Paste the code")
                            LumiaPrimaryButton("Sign in with the code", enabled = !busy && magicCode.isNotBlank(), modifier = Modifier.fillMaxWidth()) { scope.launch { completeMagicLink() } }
                        } else {
                            LumiaLinkButton("Email me a sign-in link", enabled = !busy && handle.isNotBlank()) { scope.launch { requestMagicLink() } }
                        }
                    }
                    if (!creating && current.identityProofSignInEnabled) {
                        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12), verticalAlignment = Alignment.CenterVertically) {
                            Box(Modifier.weight(1f).height(1.dp).background(LumiaTokens.line.color))
                            Text("or", style = LumiaTokens.textSection.style, color = LumiaTokens.inkSecondary.color)
                            Box(Modifier.weight(1f).height(1.dp).background(LumiaTokens.line.color))
                        }
                        LumiaSecondaryButton("Link this phone from another device", modifier = Modifier.fillMaxWidth()) { store.route = Route.LINK }
                    }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space4), verticalAlignment = Alignment.CenterVertically) {
                if (creating) {
                    Text("Already have an account?", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                    LumiaLinkButton("Sign in") { creating = false; error = "" }
                } else {
                    Text("New to Lumia?", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
                    if (current.openRegistration) LumiaLinkButton("Create an account") { creating = true; error = "" }
                    else Text("This instance is not open for new accounts.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color, textAlign = TextAlign.Center)
                }
            }
        }
        Spacer(Modifier.height(LumiaTokens.space24))
    }
}
