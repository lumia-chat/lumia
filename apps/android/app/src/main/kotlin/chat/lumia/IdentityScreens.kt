// Setting up private messages on a phone (identity spec 4.1, 4.3, 4.7;
// issue #20; the "Setting up private messages" phone boards): Home's
// reminder and Devices' card, the recovery phrase and its three-word
// check, "Private messages need this phone" with Link, Restore and Start
// over, the restore and start-over screens, the reauth they ask for, and
// approving another device from this one: by a prompt (4.2.1) or a pasted
// request (4.2). The rules they draw by
// are lumiakit's `Identity.kt`.
package chat.lumia

import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Block
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Devices
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Language
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.SmartToy
import androidx.compose.material.icons.filled.WarningAmber
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.ModalBottomSheetProperties
import androidx.compose.material3.SheetValue
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import chat.lumia.kit.AppStore
import chat.lumia.kit.LINK_APPROVE_WARNING
import chat.lumia.kit.LINK_CODE_HOW
import chat.lumia.kit.LumiaDangerButton
import chat.lumia.kit.LumiaEmptyState
import chat.lumia.kit.LumiaErrorText
import chat.lumia.kit.LumiaField
import chat.lumia.kit.LumiaLinkButton
import chat.lumia.kit.LumiaNotice
import chat.lumia.kit.LumiaPrimaryButton
import chat.lumia.kit.LumiaSecondaryButton
import chat.lumia.kit.LumiaSession
import chat.lumia.kit.LumiaTokens
import chat.lumia.kit.LumiaWarning
import chat.lumia.kit.NOT_A_LINK_REQUEST
import chat.lumia.kit.PROMPT_STOPPED
import chat.lumia.kit.PHRASE_FILE_NAME
import chat.lumia.kit.PHRASE_KEEP
import chat.lumia.kit.PHRASE_SAVE_CLOUD
import chat.lumia.kit.PHRASE_SAVE_TITLE
import chat.lumia.kit.PHRASE_SAVE_WARNING
import chat.lumia.kit.PHRASE_WARNING
import chat.lumia.kit.PhraseQuiz
import chat.lumia.kit.PrivateMessages
import chat.lumia.kit.PromptStage
import chat.lumia.kit.REAUTH_RETURN
import chat.lumia.kit.Route
import chat.lumia.kit.completionsLabel
import chat.lumia.kit.deviceLabel
import chat.lumia.kit.keyChangeWords
import chat.lumia.kit.linkCodeForReading
import chat.lumia.kit.linkCodeSpoken
import chat.lumia.kit.linkDigitsSpoken
import chat.lumia.kit.linkRequestMeta
import chat.lumia.kit.linkRequestOf
import chat.lumia.kit.linkRequestWho
import chat.lumia.kit.linkRequestWorthAsking
import chat.lumia.kit.lumiaCode
import chat.lumia.kit.lumiaDetails
import chat.lumia.kit.lumiaMessage
import chat.lumia.kit.personalKinds
import chat.lumia.kit.phraseColumns
import chat.lumia.kit.phraseCompletions
import chat.lumia.kit.phraseCount
import chat.lumia.kit.phraseDocument
import chat.lumia.kit.phraseOf
import chat.lumia.kit.phraseWords
import chat.lumia.kit.privateContacts
import chat.lumia.kit.promptLine
import chat.lumia.kit.promptStageOf
import chat.lumia.kit.promptTitle
import chat.lumia.kit.readLinkRequest
import chat.lumia.kit.restoreReady
import kotlinx.coroutines.launch
import uniffi.lumia_ffi.LinkRequestSummary
import uniffi.lumia_ffi.LumiaException

// Home and Devices

/**
 * Home's reminder (identity spec 4.1 step 7), over the conversations while
 * the phrase waits to be confirmed and it was not put off: "Save it now",
 * or "In a week", which puts it off for seven days on this phone.
 */
@Composable
fun PhraseReminder(save: () -> Unit, later: () -> Unit) {
    LumiaWarning(Icons.Filled.WarningAmber, title = "Save your recovery phrase", text = "It's the only way back to your private messages if you lose this phone.") {
        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
            LumiaPrimaryButton("Save it now", Modifier.weight(1f), onClick = save)
            LumiaSecondaryButton("In a week", onClick = later)
        }
    }
}

/** Devices' standing card while the phrase waits to be confirmed; never put off. */
@Composable
fun PhraseNotSaved(save: () -> Unit) {
    LumiaWarning(Icons.Filled.WarningAmber, title = "Recovery phrase not saved", text = "If you lose this phone before you save it, your private messages can't be recovered.") {
        LumiaPrimaryButton("Save your recovery phrase", Modifier.fillMaxWidth(), onClick = save)
    }
}

// The phrase

/** A screen's content over a footer of its actions, the line between them, as the boards' phrase, restore and start-over screens have it. */
@Composable
private fun WithFooter(content: @Composable ColumnScope.() -> Unit, footer: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().imePadding()) {
        Column(
            Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()).padding(LumiaTokens.space16),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space14),
            content = content,
        )
        Box(Modifier.fillMaxWidth().height(1.dp).background(LumiaTokens.line.color))
        Column(
            Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = LumiaTokens.space16, vertical = LumiaTokens.space12),
            verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
            content = footer,
        )
    }
}

/**
 * The recovery phrase (step 1 of 2), shown only after the phone's own
 * unlock and kept out of screenshots and the recents view: the warning,
 * the 24 words numbered in two columns, Print, Save as a file (behind the
 * warning sheet) and "I've written them down", which is the check. No
 * copy. [again] is a check that had a word wrong.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PhraseScreen(session: LumiaSession, again: Boolean, from: String, back: () -> Unit, check: () -> Unit) {
    SecureWindow()
    val context = LocalContext.current
    var phrase by remember { mutableStateOf<String?>(null) }
    var loaded by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf("") }
    var saving by remember { mutableStateOf(false) }
    val unlock = rememberDeviceUnlock("Show your recovery phrase", "Unlock to see the words") { session.phraseUnlocked = it }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri ->
        val words = phrase ?: return@rememberLauncherForActivityResult
        if (uri == null) return@rememberLauncherForActivityResult
        val saved = savePhrase(context, uri, phraseDocument(phraseWords(words)))
        Toast.makeText(context, if (saved) "Saved." else "The file could not be written.", Toast.LENGTH_SHORT).show()
    }
    // Asked once as the screen opens; a turn of the phone keeps the unlock (the session's) and asks nothing.
    LaunchedEffect(Unit) { if (!session.phraseUnlocked) unlock() }
    LaunchedEffect(session.phraseUnlocked) {
        if (!session.phraseUnlocked) return@LaunchedEffect
        try {
            phrase = session.identityPhrase()
        } catch (e: Exception) {
            error = e.lumiaMessage
        } finally {
            loaded = true
        }
    }

    PanelScreen(back = from, onBack = back, title = "Recovery phrase", subtitle = "Step 1 of 2") {
        val words = phrase?.let(::phraseWords)
        when {
            !session.phraseUnlocked -> Box(Modifier.fillMaxSize().padding(LumiaTokens.space16), contentAlignment = Alignment.Center) {
                LumiaEmptyState(Icons.Filled.Lock, "Unlock to see your phrase", "Your phone's own unlock keeps the words from anyone else holding it.", action = "Unlock" to unlock)
            }
            !loaded -> Unit
            words == null -> Box(Modifier.fillMaxSize().padding(LumiaTokens.space16), contentAlignment = Alignment.Center) {
                if (error.isNotEmpty()) LumiaErrorText(error)
                else LumiaEmptyState(Icons.Filled.Lock, "Your phrase is saved", "You confirmed it, so this phone no longer keeps a copy.")
            }
            else -> WithFooter(
                content = {
                    if (again) LumiaErrorText("One of those words wasn't right. Check what you wrote against the phrase, then try again.")
                    LumiaWarning(Icons.Filled.WarningAmber, PHRASE_WARNING)
                    PhraseGrid(words)
                    Text(PHRASE_KEEP, style = LumiaTokens.textCompact.style, color = LumiaTokens.inkSecondary.color)
                    Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                        LumiaSecondaryButton("Print", Modifier.weight(1f)) { printPhrase(context, words) }
                        LumiaSecondaryButton("Save as a file", Modifier.weight(1f)) { saving = true }
                    }
                },
                footer = { LumiaPrimaryButton("I've written them down", Modifier.fillMaxWidth(), onClick = check) },
            )
        }
    }

    // Saving asks first: the file is the identity, and a synced folder is a cloud's copy of it.
    if (saving) {
        ModalBottomSheet(onDismissRequest = { saving = false }, containerColor = LumiaTokens.surface.color) {
            SecureWindow()
            Column(Modifier.padding(start = LumiaTokens.space24, end = LumiaTokens.space24, bottom = LumiaTokens.space40), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space18)) {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                    Text(PHRASE_SAVE_TITLE, style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color)
                    Text(PHRASE_SAVE_WARNING, style = LumiaTokens.textSubtitle.style, color = LumiaTokens.inkMuted.color)
                }
                LumiaWarning(Icons.Filled.WarningAmber, PHRASE_SAVE_CLOUD)
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                    LumiaPrimaryButton("Save anyway", Modifier.fillMaxWidth()) {
                        saving = false
                        save.launch(PHRASE_FILE_NAME)
                    }
                    LumiaSecondaryButton("Write it down instead", Modifier.fillMaxWidth()) { saving = false }
                }
            }
        }
    }
}

/** The words, numbered in two columns read down; to a screen reader a list, each "1, orbit". */
@Composable
private fun PhraseGrid(words: List<String>) {
    Row(
        Modifier.fillMaxWidth().semantics { contentDescription = "Your ${words.size} words, in order" },
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space6),
    ) {
        for (column in phraseColumns(words)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                for ((n, word) in column) {
                    Row(
                        Modifier.fillMaxWidth()
                            .background(LumiaTokens.inset.color, RoundedCornerShape(LumiaTokens.radius10))
                            .semantics(mergeDescendants = true) { contentDescription = "$n, $word" }
                            .padding(horizontal = LumiaTokens.space10, vertical = LumiaTokens.space6),
                        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("$n", style = LumiaTokens.textTag.style, color = LumiaTokens.inkMuted.color, textAlign = TextAlign.End, modifier = Modifier.width(LumiaTokens.space24))
                        Text(word, style = MONO_WORD, color = LumiaTokens.ink.color)
                    }
                }
            }
        }
    }
}

/** A phrase word as the boards set it: mono, a step over the body, medium. */
private val MONO_WORD
    @Composable get() = LumiaTokens.textSubtitle.style.copy(fontFamily = FontFamily.Monospace, fontWeight = FontWeight(500))

/**
 * Typed words a keyboard should not learn or offer again: no suggestions,
 * no correction, no capitals (the phrase's wordlist is lower case).
 */
private fun phraseKeyboard(ime: ImeAction) = KeyboardOptions(
    capitalization = KeyboardCapitalization.None,
    autoCorrectEnabled = false,
    keyboardType = KeyboardType.Password,
    imeAction = ime,
)

/**
 * "Check your phrase" (step 2 of 2): three of the 24 words, at the
 * positions the core chose, each typed with the wordlist completing it.
 * Right ends the reminders ([done]); a word wrong shows the phrase again
 * ([wrong]), and the next check asks other words. What is typed is not
 * kept when Android saves the screen: a saved state can outlast the
 * process on disk.
 */
@Composable
fun PhraseCheckScreen(session: LumiaSession, back: () -> Unit, wrong: () -> Unit, done: () -> Unit) {
    SecureWindow()
    val scope = rememberCoroutineScope()
    val focus = LocalFocusManager.current
    var quiz by remember { mutableStateOf<PhraseQuiz?>(null) }
    var focused by remember { mutableStateOf(-1) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf("") }
    LaunchedEffect(Unit) {
        try { quiz = PhraseQuiz(session.identityPhraseQuiz()) } catch (e: Exception) { error = e.lumiaMessage }
    }

    fun confirm() {
        val q = quiz ?: return
        if (!q.ready || busy) return
        scope.launch {
            busy = true
            error = ""
            try {
                if (session.identityPhraseConfirm(q.words)) done() else wrong()
            } catch (e: Exception) {
                error = e.lumiaMessage
            } finally {
                busy = false
            }
        }
    }

    PanelScreen(back = "the phrase", onBack = back, title = "Check your phrase", subtitle = "Step 2 of 2") {
        WithFooter(
            content = {
                Text("Type these three words from what you wrote down, to make sure it's right.", style = LumiaTokens.textCompact.style, color = LumiaTokens.inkSecondary.color)
                val q = quiz
                q?.positions?.forEachIndexed { i, position ->
                    val last = i == q.positions.lastIndex
                    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space4)) {
                        WordField("Word $position", q.words[i], { quiz = quiz?.typed(i, it) }, Modifier.onFocusChanged { if (it.isFocused) focused = i }, if (last) ImeAction.Done else ImeAction.Next) {
                            if (last) confirm() else focus.moveFocus(FocusDirection.Down)
                        }
                        val offered = if (focused == i) q.offered(i, phraseCompletions(q.words[i])) else emptyList()
                        if (offered.isNotEmpty()) {
                            Completions(q.words[i], offered) { word ->
                                quiz = quiz?.typed(i, word)
                                if (last) focus.clearFocus() else focus.moveFocus(FocusDirection.Down)
                            }
                        }
                    }
                }
            },
            footer = {
                // Over the actions, where it shows above the keyboard too.
                if (error.isNotEmpty()) LumiaErrorText(error)
                LumiaPrimaryButton("Confirm", Modifier.fillMaxWidth(), enabled = quiz?.ready == true && !busy) { confirm() }
                LumiaLinkButton("Show the phrase again", Modifier.fillMaxWidth(), onClick = back)
            },
        )
    }
}

/** One word's field: the label over it, mono, nothing a keyboard learns. */
@Composable
private fun WordField(label: String, text: String, onText: (String) -> Unit, modifier: Modifier, ime: ImeAction, next: () -> Unit) {
    Text(label, style = LumiaTokens.textLabel.style, color = LumiaTokens.inkEmphasis.color)
    Box(
        Modifier.fillMaxWidth().heightIn(min = LumiaTokens.controlHeight)
            .background(LumiaTokens.field.color, RoundedCornerShape(LumiaTokens.radius12))
            .border(1.dp, LumiaTokens.borderForm.color, RoundedCornerShape(LumiaTokens.radius12))
            .padding(horizontal = LumiaTokens.space14),
        contentAlignment = Alignment.CenterStart,
    ) {
        PhraseInput(text, onText, "", modifier.fillMaxWidth().semantics { contentDescription = label }, maxLines = 1, ime = ime, next = next)
    }
}

/** The text field the phrase is typed into: mono, with the phrase keyboard. */
@Composable
private fun PhraseInput(text: String, onText: (String) -> Unit, placeholder: String, modifier: Modifier, maxLines: Int, ime: ImeAction, next: () -> Unit) {
    androidx.compose.foundation.text.BasicTextField(
        value = text,
        onValueChange = onText,
        modifier = modifier,
        textStyle = MONO_WORD.copy(color = LumiaTokens.inkField.color),
        cursorBrush = androidx.compose.ui.graphics.SolidColor(LumiaTokens.accent.color),
        maxLines = maxLines,
        singleLine = maxLines == 1,
        keyboardOptions = phraseKeyboard(ime),
        keyboardActions = KeyboardActions(onNext = { next() }, onDone = { next() }),
        decorationBox = { inner ->
            Box {
                if (text.isEmpty()) Text(placeholder, style = LumiaTokens.textBody.style, color = LumiaTokens.inkMuted.color)
                inner()
            }
        },
    )
}

/** The wordlist's completions under a field: rows a finger's height, a list to a screen reader named for what was typed. */
@Composable
private fun Completions(prefix: String, words: List<String>, pick: (String) -> Unit) {
    Column(
        Modifier.fillMaxWidth().padding(top = LumiaTokens.space4)
            .background(LumiaTokens.surface.color, RoundedCornerShape(LumiaTokens.radius12))
            .border(1.dp, LumiaTokens.borderForm.color, RoundedCornerShape(LumiaTokens.radius12))
            .semantics { contentDescription = completionsLabel(prefix) }
            .padding(LumiaTokens.space4),
    ) {
        for (word in words) {
            Box(
                Modifier.fillMaxWidth().heightIn(min = LumiaTokens.tileSize)
                    .clickable { pick(word) }
                    .padding(horizontal = LumiaTokens.space14),
                contentAlignment = Alignment.CenterStart,
            ) { Text(word, style = MONO_WORD, color = LumiaTokens.ink.color) }
        }
    }
}

// Private messages need this phone

/**
 * Drawn wherever a private message would start or be accepted on a phone
 * that cannot send one, and where the app used to send it to "Link this
 * phone": for an account with private messages on another device, Link
 * (the existing flow), Restore with the phrase, and, last, Start over; for
 * an account with none, setting up here under a reauth, since the sign-in
 * that would have carried one is gone.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NeedsSetupSheet(store: AppStore, session: LumiaSession, push: (Screen) -> Unit, dismiss: () -> Unit) {
    LaunchedEffect(Unit) { session.refreshPrivateMessages() }
    LaunchedEffect(session.privateMessages) { if (session.privateMessages == PrivateMessages.READY) dismiss() }
    // Open all the way: the reauth's field brings the keyboard up, which a half-open sheet would sit under.
    ModalBottomSheet(onDismissRequest = dismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), containerColor = LumiaTokens.surface.color) {
        Column(Modifier.padding(start = LumiaTokens.space24, end = LumiaTokens.space24, bottom = LumiaTokens.space40).imePadding(), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space18)) {
            if (session.privateMessages == PrivateMessages.SET_UP) SetUpHere(store, session, dismiss)
            else {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                    Text("Private messages need this phone", style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color)
                    Text("Your account has private messages on another device. Bring this phone in to read and send them here.", style = LumiaTokens.textSubtitle.style, color = LumiaTokens.inkMuted.color)
                }
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                    LumiaPrimaryButton("Link from a device you use", Modifier.fillMaxWidth()) { dismiss(); store.route = Route.LINK }
                    LumiaSecondaryButton("Restore with your recovery phrase", Modifier.fillMaxWidth()) { dismiss(); push(Screen.Restore) }
                    LumiaLinkButton("Lost every device and the phrase? Start over", Modifier.fillMaxWidth()) { dismiss(); push(Screen.StartOver) }
                }
            }
        }
    }
}

/** The sheet for an account with no identity met after its sign-in: a reauth, then set up as sign-up does. */
@Composable
private fun SetUpHere(store: AppStore, session: LumiaSession, done: () -> Unit) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val draft = remember { ReauthDraft() }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf("") }

    fun setUp(prove: Boolean) {
        scope.launch {
            busy = true
            error = ""
            try {
                if (prove) draft.prove(session)
                session.identitySetUp(deviceLabel)
                Toast.makeText(context, "Private messages are set up on this phone.", Toast.LENGTH_SHORT).show()
                done()
            } catch (e: Exception) {
                error = setupError(e, draft)
            } finally {
                busy = false
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
        Text("Set up private messages", style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color)
        Text("Confirm it's you, and private messages are set up on this phone with your account. You'll be asked to save a recovery phrase afterwards.", style = LumiaTokens.textSubtitle.style, color = LumiaTokens.inkMuted.color)
    }
    ReauthFields(store, session, draft, enabled = !busy, onError = { error = it }) { setUp(prove = false) }
    if (error.isNotEmpty()) LumiaErrorText(error)
    LumiaPrimaryButton("Set up private messages", Modifier.fillMaxWidth(), enabled = draft.ready && !busy) { setUp(prove = true) }
}

// Restore

/**
 * Restore with the recovery phrase (identity spec 4.3): the 24 words in
 * one field with their count, what restoring does to the other devices
 * and to messages from before, and Restore once all 24 are in. It signs
 * in by the identity, so it is offered before a sign-in too. Nothing
 * typed is kept when Android saves the screen, and the screen is kept
 * out of screenshots.
 */
@Composable
fun RestoreScreen(store: AppStore, session: LumiaSession, from: String, back: () -> Unit, done: () -> Unit) {
    SecureWindow()
    val scope = rememberCoroutineScope()
    var text by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf("") }
    val ready = restoreReady(text)

    fun restore() {
        if (!ready || busy) return
        scope.launch {
            busy = true
            error = ""
            try {
                val phrase = phraseOf(text)
                store.signIn { identityRecover(phrase, deviceLabel) }
                done()
            } catch (e: Exception) {
                error = "Not restored: ${e.lumiaMessage}"
            } finally {
                busy = false
            }
        }
    }

    PanelScreen(back = from, onBack = back, title = "Restore", subtitle = "With your recovery phrase") {
        WithFooter(
            content = {
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("Your 24 words, in order", style = LumiaTokens.textLabel.style, color = LumiaTokens.inkEmphasis.color, modifier = Modifier.weight(1f))
                        Text(phraseCount(text), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                    }
                    Box(
                        Modifier.fillMaxWidth().heightIn(min = LumiaTokens.controlHeight * 3)
                            .background(LumiaTokens.field.color, RoundedCornerShape(LumiaTokens.radius12))
                            .border(1.dp, LumiaTokens.borderForm.color, RoundedCornerShape(LumiaTokens.radius12))
                            .padding(LumiaTokens.space12),
                    ) {
                        PhraseInput(text, { text = it }, "orbit cactus timber …", Modifier.fillMaxWidth().semantics { contentDescription = "Your 24 words, in order" }, maxLines = 8, ime = ImeAction.Done) { restore() }
                    }
                }
                LumiaWarning(Icons.Filled.WarningAmber, "Restoring makes this phone your identity's only device. Every other device of yours stops reading private messages, so a lost one can't keep reading them.")
                LumiaNotice(Icons.Filled.History, text = "Messages from before can't be read here unless history backup was on.")
            },
            footer = {
                // Over the actions, where it shows above the keyboard too.
                if (error.isNotEmpty()) LumiaErrorText(error)
                LumiaPrimaryButton(if (busy) "Restoring…" else "Restore", Modifier.fillMaxWidth(), enabled = ready && !busy) { restore() }
                if (!ready) Text("Restore turns on once all 24 words are in.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkMuted.color, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
            },
        )
    }
}

// Start over

/**
 * Start over (identity spec 4.7), for someone with neither a device of the
 * identity nor its phrase: what stays and what is lost, who will see a key
 * change (only those this account talks to privately), the warning, a
 * reauth, and the danger button. [done] is told whether a new phrase
 * waits to be saved.
 */
@Composable
fun StartOverScreen(store: AppStore, session: LumiaSession, from: String, back: () -> Unit, done: (phrase: Boolean) -> Unit) {
    val scope = rememberCoroutineScope()
    val draft = remember { ReauthDraft() }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf("") }
    val people = privateContacts(
        (session.snapshot?.channels ?: emptyList()).filter { it.kind in personalKinds }.map { it.participantAccountIds },
        session.me,
    )

    fun startOver(prove: Boolean) {
        scope.launch {
            busy = true
            error = ""
            try {
                if (prove) draft.prove(session)
                session.identityStartOver(deviceLabel)
                done(session.phraseUnconfirmed)
            } catch (e: Exception) {
                error = when {
                    e.lumiaCode == "ALREADY_EXISTS" -> "This phone's identity belongs to another account on this instance."
                    else -> setupError(e, draft)
                }
            } finally {
                busy = false
            }
        }
    }

    PanelScreen(back = from, onBack = back, title = "Start over", subtitle = "With a new identity") {
        WithFooter(
            content = {
                Text("Without a device or your recovery phrase, your old identity can't come back. You can start over on this phone with a new one.", style = LumiaTokens.textBody.style, color = LumiaTokens.inkBody.color)
                Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                    for (line in listOf(
                        "Your account, servers and channels stay as they are.",
                        "Private messages from before stay unreadable.",
                        keyChangeWords(people),
                        "You'll get a new recovery phrase to save.",
                    )) {
                        Row(horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                            Text("•", style = LumiaTokens.textCompact.style, color = LumiaTokens.inkMuted.color)
                            Text(line, style = LumiaTokens.textCompact.style, color = LumiaTokens.inkBody.color, modifier = Modifier.weight(1f))
                        }
                    }
                }
                LumiaWarning(Icons.Filled.WarningAmber, "Only do this if the old phrase is truly gone. If you find it later, your old identity can be restored, but not onto this account.")
                ReauthFields(store, session, draft, enabled = !busy, onError = { error = it }) { startOver(prove = false) }
            },
            footer = {
                // Over the actions, where it shows above the keyboard too.
                if (error.isNotEmpty()) LumiaErrorText(error)
                LumiaDangerButton("Start over with a new identity", Modifier.fillMaxWidth(), enabled = draft.ready && !busy) { startOver(prove = true) }
                LumiaSecondaryButton("Keep looking for the phrase", Modifier.fillMaxWidth(), onClick = back)
            },
        )
    }
}

// Reauth (identity spec 10)

/**
 * A credential proved again before setting up or starting over: the
 * password, or a code mailed to the account's address; the provider's
 * login, where the instance has one, is a button of its own and needs
 * neither.
 */
@Stable
class ReauthDraft {
    var mailed by mutableStateOf(false)
    var sent by mutableStateOf(false)
    var password by mutableStateOf("")
    var code by mutableStateOf("")

    val ready: Boolean get() = if (mailed) code.isNotBlank() else password.isNotEmpty()

    suspend fun prove(session: LumiaSession) {
        if (mailed) session.reauthMailed(code.trim()) else session.reauthPassword(password)
    }
}

/** What a refused setup, start over or reauth reads as. */
private fun setupError(e: Exception, draft: ReauthDraft): String = when {
    e.lumiaCode == "UNAUTHENTICATED" -> if (draft.mailed) "That code was not accepted." else "That password was not accepted."
    e.lumiaCode == "RATE_LIMITED" -> "Too many tries. Wait a little, then try again."
    e is LumiaException.AccountHasIdentity -> "This account already has private messages on another device."
    e is LumiaException.ReauthNeeded -> "Confirm it's you first."
    else -> e.lumiaMessage
}

/**
 * The reauth's fields: "Confirm with your password", or the mailed code
 * once asked for; and the provider's button where the instance has one,
 * whose login comes back on [REAUTH_RETURN] and goes straight to
 * [proved] with the token it carried.
 */
@Composable
private fun ReauthFields(store: AppStore, session: LumiaSession, draft: ReauthDraft, enabled: Boolean, onError: (String) -> Unit, proved: () -> Unit) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var provider by remember { mutableStateOf("") }
    LaunchedEffect(Unit) { provider = runCatching { session.instanceInfo().oidcName }.getOrDefault("") }
    // Back from the provider's login with a reauth token on the app's own scheme.
    LaunchedEffect(store.browserReturn) {
        val back = store.browserReturn ?: return@LaunchedEffect
        if (!back.startsWith(REAUTH_RETURN)) return@LaunchedEffect
        store.browserReturn = null
        session.reauthTake(back)
        proved()
    }

    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
        if (draft.mailed) {
            LumiaField("Code from the email", draft.code, { draft.code = it }, placeholder = "6 digits", keyboard = KeyboardOptions(keyboardType = KeyboardType.Number))
            LumiaLinkButton("Use your password instead", enabled = enabled) { draft.mailed = false }
        } else {
            LumiaField("Confirm with your password", draft.password, { draft.password = it }, placeholder = "Your password", secure = true)
            LumiaLinkButton("Email me a code instead", enabled = enabled) {
                scope.launch {
                    try {
                        session.reauthMailCode()
                        draft.mailed = true
                        draft.sent = true
                    } catch (e: Exception) {
                        onError(
                            when {
                                e.lumiaDetails.values.any { it == "no_email" } -> "This account has no email address to send a code to."
                                e.lumiaCode == "RATE_LIMITED" -> "Too many codes in the last hour; try later."
                                else -> e.lumiaMessage
                            },
                        )
                    }
                }
            }
        }
        if (draft.mailed && draft.sent) Text("A code is on its way to your email address.", style = LumiaTokens.textMeta.style, color = LumiaTokens.inkSecondary.color)
        if (provider.isNotEmpty()) {
            LumiaSecondaryButton("Confirm with $provider", Modifier.fillMaxWidth(), enabled = enabled) {
                scope.launch {
                    try { openInBrowser(context, session.reauthOidcUrl()) } catch (e: Exception) { onError(e.lumiaMessage) }
                }
            }
        }
    }
}

// Approve a device

/**
 * Approve a device (identity spec 4.2 steps 2 to 6; the Devices board's
 * "Link another device"), from Devices on a phone that holds the
 * identity, so it can bring in a laptop, a browser or another phone: the
 * request the new device shows, pasted; what it would add, read by the
 * core with nothing sent, or "That isn't a link request"; the warning;
 * Approve, behind the phone's own unlock as the phrase is; then the code,
 * large and in two groups to read out, for the new device to claim with.
 * The approval is the session's, so the phone turned or the screen left
 * while it runs keeps the code. The request is kept in the saved state:
 * it is the new device's public half and opens nothing by itself.
 */
@Composable
fun ApproveScreen(session: LumiaSession, from: String, back: () -> Unit) {
    val clipboard = LocalClipboardManager.current
    val focus = LocalFocusManager.current
    val approval = session.approval
    var text by rememberSaveable { mutableStateOf(session.approval?.request ?: "") }
    val request = linkRequestOf(text)
    val summary = remember(request) { if (linkRequestWorthAsking(request)) readLinkRequest(request) else null }
    val unlock = rememberDeviceUnlock("Approve a device", "Unlock to let it read your private messages") { ok -> if (ok) session.approve(request) }
    val code = approval?.code
    // The back button is this screen's own, so an answered approval is forgotten on the way out.
    BackHandler(onBack = back)

    PanelScreen(back = from, onBack = back, title = "Approve a device", subtitle = if (code == null) "Link another device" else "Approved") {
        if (code != null) {
            WithFooter(
                content = {
                    Column(
                        Modifier.fillMaxWidth().padding(vertical = LumiaTokens.space24),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
                    ) {
                        Text("Code for the new device", style = LumiaTokens.textLabel.style, color = LumiaTokens.inkSecondary.color)
                        Text(
                            linkCodeForReading(code),
                            style = LumiaTokens.titleAuth.style.copy(fontFamily = FontFamily.Monospace),
                            color = LumiaTokens.ink.color,
                            modifier = Modifier.semantics { contentDescription = linkCodeSpoken(code) },
                        )
                    }
                    Text(LINK_CODE_HOW, style = LumiaTokens.textBody.style, color = LumiaTokens.inkSecondary.color, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
                },
                footer = { LumiaPrimaryButton("Done", Modifier.fillMaxWidth(), onClick = back) },
            )
        } else {
            val underWay = approval?.underWay == true
            WithFooter(
                content = {
                    Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text("Paste the request the new device shows", style = LumiaTokens.textLabel.style, color = LumiaTokens.inkEmphasis.color, modifier = Modifier.weight(1f))
                            LumiaLinkButton("Paste", enabled = !underWay) { clipboard.getText()?.text?.let { text = it } }
                        }
                        Box(
                            Modifier.fillMaxWidth().heightIn(min = LumiaTokens.controlHeight * 3)
                                .background(LumiaTokens.field.color, RoundedCornerShape(LumiaTokens.radius12))
                                .border(1.dp, LumiaTokens.borderForm.color, RoundedCornerShape(LumiaTokens.radius12))
                                .padding(LumiaTokens.space12),
                            // The field fills the box, so a tap anywhere in it, not only on the first line, brings the keyboard.
                            propagateMinConstraints = true,
                        ) {
                            PhraseInput(text, { if (!underWay) text = it }, "CiQ3…", Modifier.fillMaxWidth().semantics { contentDescription = "The request the new device shows" }, maxLines = 8, ime = ImeAction.Done) { focus.clearFocus() }
                        }
                    }
                    when {
                        request.isEmpty() -> LumiaNotice(Icons.Filled.Devices, text = "On the new device, choose Link from a device you use, then Link another way and Start linking, and copy the request it shows.")
                        summary == null -> LumiaErrorText(NOT_A_LINK_REQUEST)
                        else -> {
                            LinkRequestRow(summary)
                            LumiaWarning(Icons.Filled.WarningAmber, LINK_APPROVE_WARNING)
                        }
                    }
                },
                footer = {
                    // Over the actions, where it shows above the keyboard too; only while it is about the request in the field.
                    approval?.error?.takeIf { approval.request == request }?.let { LumiaErrorText(it) }
                    LumiaPrimaryButton(if (underWay) "Approving…" else "Approve", Modifier.fillMaxWidth(), enabled = summary != null && !underWay) {
                        focus.clearFocus()
                        unlock()
                    }
                },
            )
        }
    }
}

// A device asking to be linked (identity spec 4.2.1)

/**
 * The prompt a link session of the account raises on this phone, which
 * holds the identity, over whatever is on screen, one at a time and oldest
 * first (the session keeps which): what would be added, Continue or Not
 * me; waiting for the new device; the six digits, They match (behind the
 * phone's own unlock, as Approve a device is) or They don't match; a
 * request that can't be trusted, with Not me alone; and once approved,
 * "Pixel 9 is linked." for a moment. It goes by its buttons or when its
 * session does, not by a swipe: Not me is how to say no, and a prompt put
 * aside here would only come back at the next account event.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LinkPromptSheet(session: LumiaSession) {
    // The session approved once the unlock answers, which may be after the prompt moved on.
    var approving by remember { mutableStateOf<String?>(null) }
    val unlock = rememberDeviceUnlock("Approve a device", "Unlock to let it read your private messages") { ok ->
        approving?.let { if (ok) session.promptConfirm(it) }
        approving = null
    }
    val prompt = session.prompt
    val done = session.promptDone
    if (prompt == null && done == null) return
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true, confirmValueChange = { it != SheetValue.Hidden })

    // Neither a swipe, a tap on the scrim nor the back button puts it away: hidden, it would stay over the app with nothing to show.
    ModalBottomSheet(
        onDismissRequest = {},
        sheetState = state,
        containerColor = LumiaTokens.surface.color,
        properties = ModalBottomSheetProperties(shouldDismissOnBackPress = false),
    ) {
        Column(Modifier.padding(start = LumiaTokens.space24, end = LumiaTokens.space24, bottom = LumiaTokens.space40), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space18)) {
            if (done != null || prompt == null) {
                Row(
                    Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
                    horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space10),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    val linked = done != PROMPT_STOPPED
                    Icon(if (linked) Icons.Filled.CheckCircle else Icons.Filled.Block, contentDescription = null, tint = if (linked) LumiaTokens.accent.color else LumiaTokens.inkMuted.color, modifier = Modifier.size(LumiaTokens.space24))
                    Text(done.orEmpty(), style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color)
                }
                return@Column
            }
            val stage = promptStageOf(prompt.stage)
            val r = prompt.request
            val id = prompt.sessionId
            val busy = session.promptBusy || approving != null
            Column(Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }, verticalArrangement = Arrangement.spacedBy(LumiaTokens.space6)) {
                Text(promptTitle(stage, r.kind, r.label, r.webOrigin), style = LumiaTokens.titleChannel.style, color = LumiaTokens.ink.color)
                promptLine(stage)?.let { Text(it, style = LumiaTokens.textSubtitle.style, color = if (stage == PromptStage.REFUSED) LumiaTokens.dnd.color else LumiaTokens.inkMuted.color) }
            }
            when (stage) {
                PromptStage.NEW -> LinkRequestRow(r)
                PromptStage.WAITING -> CircularProgressIndicator(Modifier.size(LumiaTokens.space18), color = LumiaTokens.inkMuted.color, strokeWidth = LumiaTokens.space2)
                PromptStage.COMPARE -> Text(
                    prompt.digits,
                    style = LumiaTokens.titleAuth.style.copy(fontFamily = FontFamily.Monospace),
                    color = LumiaTokens.ink.color,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().padding(vertical = LumiaTokens.space12).semantics { contentDescription = linkDigitsSpoken(prompt.digits) },
                )
                PromptStage.REFUSED -> LinkRequestRow(r)
            }
            session.promptFailure?.let { LumiaErrorText(it) }
            Column(verticalArrangement = Arrangement.spacedBy(LumiaTokens.space8)) {
                when (stage) {
                    PromptStage.NEW -> {
                        LumiaPrimaryButton("Continue", Modifier.fillMaxWidth(), enabled = !busy) { session.promptAnswer(id) }
                        LumiaSecondaryButton("Not me", Modifier.fillMaxWidth(), enabled = !busy) { session.promptDecline(id) }
                    }
                    PromptStage.COMPARE -> {
                        LumiaPrimaryButton(if (busy) "Approving…" else "They match", Modifier.fillMaxWidth(), enabled = !busy) {
                            approving = id
                            unlock()
                        }
                        LumiaSecondaryButton("They don't match", Modifier.fillMaxWidth(), enabled = !busy) { session.promptDecline(id, mismatch = true) }
                    }
                    PromptStage.WAITING, PromptStage.REFUSED -> LumiaSecondaryButton("Not me", Modifier.fillMaxWidth(), enabled = !busy) { session.promptDecline(id) }
                }
            }
        }
    }
}

/** What a request would add, drawn as Devices will list it: its kind's glyph, who, and its id's first eight. */
@Composable
private fun LinkRequestRow(summary: LinkRequestSummary) {
    Row(
        Modifier.fillMaxWidth()
            .background(LumiaTokens.tile.color, RoundedCornerShape(LumiaTokens.radius14))
            .border(1.dp, LumiaTokens.line.color, RoundedCornerShape(LumiaTokens.radius14))
            .padding(horizontal = LumiaTokens.space14, vertical = LumiaTokens.space12)
            .semantics(mergeDescendants = true) {},
        horizontalArrangement = Arrangement.spacedBy(LumiaTokens.space12),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val glyph = when (summary.kind) {
            "web" -> Icons.Filled.Language
            "bot" -> Icons.Filled.SmartToy
            else -> Icons.Filled.Computer
        }
        Icon(glyph, contentDescription = null, tint = LumiaTokens.iconStrong.color, modifier = Modifier.size(LumiaTokens.space24))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(LumiaTokens.space2)) {
            Text(linkRequestWho(summary.kind, summary.label, summary.webOrigin), style = LumiaTokens.textAuthor.style, color = LumiaTokens.inkEmphasis.color)
            Text(linkRequestMeta(summary.kind, summary.label, summary.deviceId), style = LumiaTokens.textMeta.style, color = LumiaTokens.inkQuiet.color)
        }
    }
}
