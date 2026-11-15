// Setting up private messages on a phone (issue #20; architecture 5.3
// and 5.4, identity spec 4.1, 4.3 and 4.7), after the canvas's "Setting
// up private messages" boards: the Home reminder and Devices' card while
// the recovery phrase waits; the phrase, shown only after the phone's own
// unlock and hidden while the app is not in front, with Print and Save as
// a file behind its warning and no copy anywhere; the three-word check;
// "Private messages need this phone" for an account whose identity is on
// other devices, leading to Link, Restore or Start over; and confirming
// it's you, which setting up and starting over need when no sign-in token
// is in hand. The rules are LumiaKit's (`PrivateMessages.swift`).
import LocalAuthentication
import LumiaFFI
import LumiaKit
import SwiftUI
import UniformTypeIdentifiers

/// Leads to whatever this phone needs before a private message can be
/// started or accepted here: the "Private messages need this phone" sheet,
/// confirming it's you, or another try. The shell gives it.
struct SetUpPrivateMessages {
    let run: () -> Void
    func callAsFunction() { run() }
}

extension EnvironmentValues {
    @Entry var setUpPrivateMessages = SetUpPrivateMessages {}
}

// MARK: Reminders

/// The amber card the reminder and Devices' "not saved" are drawn in.
private struct WarningCard<Actions: View>: View {
    let title: String
    let text: String
    @ViewBuilder let actions: () -> Actions

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            HStack(alignment: .top, spacing: LumiaTokens.space10) {
                Image(systemName: "exclamationmark.triangle").foregroundStyle(LumiaTokens.toneAmberInk.color)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    Text(title).font(LumiaTokens.textAuthor.font).fontWeight(.bold)
                        .accessibilityAddTraits(.isHeader)
                    Text(text).font(LumiaTokens.textMeta.font)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .foregroundStyle(LumiaTokens.toneAmberInk.color)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            actions()
        }
        .padding(LumiaTokens.space12)
        .background(LumiaTokens.toneAmber.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.toneAmberInk.color.opacity(0.25)))
        .accessibilityElement(children: .contain)
    }
}

/// Home's reminder while the phrase waits: "In a week" puts it off for
/// seven days on this phone (`PhraseReminder`).
struct PhraseReminderCard: View {
    let save: () -> Void
    let later: () -> Void

    var body: some View {
        WarningCard(title: "Save your recovery phrase", text: "It's the only way back to your private messages if you lose this phone.") {
            HStack(spacing: LumiaTokens.space8) {
                Button(action: save) { Text("Save it now").frame(maxWidth: .infinity) }.buttonStyle(.lumiaPrimary)
                Button("In a week", action: later)
                    .font(LumiaTokens.textButtonSecondary.font)
                    .foregroundStyle(LumiaTokens.toneAmberInk.color)
                    .padding(.horizontal, LumiaTokens.space14)
                    .frame(minHeight: LumiaTokens.controlHeight)
                    .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius8).stroke(LumiaTokens.toneAmberInk.color.opacity(0.25)))
                    .buttonStyle(.plain)
            }
        }
    }
}

/// Devices' card while the phrase waits, never put off.
struct PhraseNotSavedCard: View {
    let save: () -> Void

    var body: some View {
        WarningCard(title: "Recovery phrase not saved", text: "If you lose this phone before you save it, your private messages can't be recovered.") {
            Button(action: save) { Text("Save your recovery phrase").frame(maxWidth: .infinity) }.buttonStyle(.lumiaPrimary)
        }
    }
}

/// What Home, Profile and a conversation say while this phone needs
/// something before private messages work here; nothing otherwise.
struct PrivateMessagesNotice: View {
    let state: PrivateMessages
    let setUp: () -> Void

    var body: some View {
        switch state {
        case .bringIn:
            LumiaNotice(icon: "lock", tone: .accent, action: ("Set up", setUp)) {
                Text("Private messages need this phone. Your account has them on another device.")
            }
        case .confirm:
            LumiaNotice(icon: "lock", tone: .accent, action: ("Set up", setUp)) {
                Text("Private messages aren't set up on this phone yet. Confirm it's you to set them up.")
            }
        case let .failed(why):
            LumiaNotice(icon: "lock", tone: .accent, action: ("Try again", setUp)) {
                Text("Private messages couldn't be set up on this phone: \(why)")
            }
        case .unknown, .ready, .setUp:
            EmptyView()
        }
    }
}

// MARK: Private messages need this phone

/// What a phone without the account's identity can do.
enum SetupChoice {
    case link, restore, startOver
}

/// Link (first), Restore, and the last resort under them.
struct NeedsSetupChoices: View {
    let choose: (SetupChoice) -> Void

    var body: some View {
        VStack(spacing: LumiaTokens.space8) {
            Button { choose(.link) } label: { Label("Link from a device you use", systemImage: "iphone").frame(maxWidth: .infinity) }
                .buttonStyle(.lumiaPrimary)
            Button { choose(.restore) } label: { Label("Restore with your recovery phrase", systemImage: "text.alignleft").frame(maxWidth: .infinity) }
                .buttonStyle(.lumiaSecondary)
            Button("Lost every device and the phrase? Start over") { choose(.startOver) }
                .buttonStyle(.lumiaLink)
                .frame(minHeight: LumiaTokens.controlHeight)
        }
    }
}

/// The sheet shown where a private message would start or be accepted, and
/// where the app used to send a phone without keys to "Link this phone".
struct NeedsSetupSheet: View {
    let choose: (SetupChoice) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space18) {
            VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                Text("Private messages need this phone").font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
                    .accessibilityAddTraits(.isHeader)
                Text("Your account has private messages on another device. Bring this phone in to read and send them here.")
                    .font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    .fixedSize(horizontal: false, vertical: true)
            }
            NeedsSetupChoices(choose: choose)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, LumiaTokens.space24)
        .padding(.top, LumiaTokens.space24)
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
        .presentationBackground(LumiaTokens.surface.color)
    }
}

// MARK: Confirm it's you

/// How the person confirms it's them (identity spec 10): the password, a
/// code from their authenticator, a code mailed to them, or a fresh login
/// at the instance's sign-in provider.
enum ReauthWay: Hashable {
    case password, authenticator, mail
}

/// The fields for it, the password first and the others offered under it.
struct ReauthFields: View {
    let session: LumiaSession
    let label: String
    @Binding var way: ReauthWay
    @Binding var secret: String
    @Binding var error: String
    /// The instance's sign-in provider, when it has one, and what to do once it has confirmed.
    let provider: String
    let viaProvider: () -> Void
    @State private var mailed = false

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            switch way {
            case .password:
                LumiaField(label, text: $secret, placeholder: "Your password", secure: true)
                    .textContentType(.password)
            case .authenticator:
                LumiaField("Code from your authenticator", text: $secret, placeholder: "6 digits")
                    .keyboardType(.numberPad).textContentType(.oneTimeCode)
            case .mail:
                if mailed {
                    LumiaField("Code from the email", text: $secret, placeholder: "6 digits")
                        .keyboardType(.numberPad).textContentType(.oneTimeCode)
                    Text("Six digits are on their way to your address. They work once, within 15 minutes.")
                        .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                } else {
                    Button { Task { await mail() } } label: { Label("Email me a code", systemImage: "envelope").frame(maxWidth: .infinity) }
                        .buttonStyle(.lumiaSecondary)
                }
            }
            if !error.isEmpty { LumiaErrorText(error) }
            // The other ways, as links under the field.
            VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                if way != .password { Button("Use your password instead") { switchTo(.password) }.buttonStyle(.lumiaLink) }
                if way != .authenticator { Button("Use a code from your authenticator") { switchTo(.authenticator) }.buttonStyle(.lumiaLink) }
                if way != .mail { Button("Email me a code instead") { switchTo(.mail) }.buttonStyle(.lumiaLink) }
                if !provider.isEmpty { Button("Confirm with \(provider)", action: viaProvider).buttonStyle(.lumiaLink) }
            }
        }
    }

    private func switchTo(_ next: ReauthWay) {
        way = next
        secret = ""
        error = ""
    }

    private func mail() async {
        error = ""
        do {
            try await session.reauthMailCode()
            mailed = true
        } catch {
            self.error = reauthRefusal(error, way: .mail)
        }
    }
}

/// Confirms it's the person by the way chosen; the token is the core's for the next call.
@MainActor func reauth(_ session: LumiaSession, way: ReauthWay, secret: String) async throws {
    switch way {
    case .password: try await session.reauthPassword(secret)
    case .authenticator: try await session.reauthCode(secret, mailed: false)
    case .mail: try await session.reauthCode(secret, mailed: true)
    }
}

/// What a refused reauth says.
func reauthRefusal(_ error: Error, way: ReauthWay) -> String {
    switch error.lumiaCode {
    case "UNAUTHENTICATED": way == .password ? "That password was not accepted." : "That code was not accepted."
    case "RATE_LIMITED": "Too many tries; wait a little and try again."
    case "FAILED_PRECONDITION" where error.lumiaDetails["state"] == "no_email": "This account has no email address to send a code to."
    case "FAILED_PRECONDITION" where error.lumiaDetails["state"] == "no_oidc": "This account doesn't sign in with that provider."
    default: error.lumiaMessage
    }
}

/// Whether the field holds enough to try.
private func canTry(_ way: ReauthWay, _ secret: String) -> Bool {
    !secret.trimmingCharacters(in: .whitespaces).isEmpty
}

/// Setting up on this phone when no sign-in token is in hand (a session
/// reopened at launch): confirm it's you, then the same silent setup.
struct SetUpScreen: View {
    @Environment(\.dismiss) private var dismiss
    let session: LumiaSession
    var back = "Home"
    @State private var way = ReauthWay.password
    @State private var secret = ""
    @State private var error = ""
    @State private var busy = false
    @State private var provider = ""

    var body: some View {
        PanelScreen(back: back, title: "Set up private messages", subtitle: "On this phone") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space14) {
                    Text("Private messages aren't set up for your account yet. Confirm it's you to set them up on this phone. You'll be asked to save a recovery phrase afterwards.")
                        .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkBody.color)
                        .fixedSize(horizontal: false, vertical: true)
                    ReauthFields(session: session, label: "Confirm with your password", way: $way, secret: $secret, error: $error, provider: provider) {
                        Task { await run(viaProvider: true) }
                    }
                }
                .padding(LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
            PanelFooter {
                Button { Task { await run(viaProvider: false) } } label: { Text("Set up private messages").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary).disabled(busy || !canTry(way, secret))
            }
        }
        .task { provider = (try? await session.instanceInfo())?.oidcName ?? "" }
    }

    private func run(viaProvider: Bool) async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            if viaProvider { try await session.reauthThroughBrowser() } else { try await reauth(session, way: way, secret: secret) }
        } catch BrowserSheetError.cancelled {
            return
        } catch {
            self.error = reauthRefusal(error, way: way)
            return
        }
        do {
            try await session.setUpPrivateMessages()
            dismiss()
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

/// Buttons fixed under a panel's scrolling content, over a rule.
struct PanelFooter<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(spacing: LumiaTokens.space8) { content() }
            .padding(.horizontal, LumiaTokens.space16)
            .padding(.top, LumiaTokens.space12)
            .padding(.bottom, LumiaTokens.space16)
            .overlay(alignment: .top) { Divider().overlay(LumiaTokens.line.color) }
    }
}

// MARK: The phrase

/// The phrase as a file the system's exporter writes: plain text.
struct PhraseDocument: FileDocument {
    static let readableContentTypes: [UTType] = [.plainText]
    let text: String

    init(text: String) { self.text = text }
    init(configuration: ReadConfiguration) throws {
        text = configuration.file.regularFileContents.flatMap { String(data: $0, encoding: .utf8) } ?? ""
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: Data(text.utf8))
    }
}

/// The 24 words, step 1 of 2: shown only after the phone's own unlock
/// (passcode, Face ID or Touch ID; straight away on a phone with none set),
/// covered while the app is not in front and forgotten when it goes to the
/// background, so the next look unlocks again. No copy: Print, or Save as
/// a file after the warning, then the check.
struct PhraseScreen: View {
    @Environment(\.scenePhase) private var scenePhase
    let session: LumiaSession
    var back = "Home"
    /// A check got a word wrong: the phrase is shown again, saying so.
    @Binding var again: Bool
    let check: () -> Void
    @State private var words: [String]?
    /// The unlock was cancelled or failed, or the app went to the background.
    @State private var locked = false
    /// This phone keeps no phrase any more: it was confirmed.
    @State private var saved = false
    @State private var warning = false
    @State private var exportAfterWarning = false
    @State private var exporting = false
    @State private var note = ""

    var body: some View {
        PanelScreen(back: back, title: "Recovery phrase", subtitle: "Step 1 of 2") {
            if let words {
                ScrollView {
                    VStack(alignment: .leading, spacing: LumiaTokens.space14) {
                        if again {
                            LumiaNotice(icon: "arrow.uturn.backward", tone: .accent) {
                                Text("One of those words wasn't right. Check what you wrote down against the phrase, then try again.")
                            }
                        }
                        LumiaNotice(icon: "exclamationmark.triangle", tone: .warning) { Text(PhraseWords.warning) }
                        wordGrid(words)
                        Text("Write them down in order, or print them, and keep them somewhere safe and offline. There's no copy button: clipboards sync to other devices, and other apps can read them.")
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                            .fixedSize(horizontal: false, vertical: true)
                        if !note.isEmpty {
                            Text(note).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        }
                    }
                    .padding(LumiaTokens.space16)
                }
                PanelFooter {
                    HStack(spacing: LumiaTokens.space8) {
                        Button { print(words) } label: { Label("Print", systemImage: "printer").frame(maxWidth: .infinity) }
                            .buttonStyle(.lumiaSecondary)
                        Button { warning = true } label: { Label("Save as a file", systemImage: "arrow.down.to.line").frame(maxWidth: .infinity) }
                            .buttonStyle(.lumiaSecondary)
                    }
                    Button { again = false; check() } label: { Text("I've written them down").frame(maxWidth: .infinity) }
                        .buttonStyle(.lumiaPrimary)
                }
                .disabled(scenePhase != .active)
            } else if saved {
                LumiaEmptyState(icon: "checkmark.shield", title: "Your recovery phrase is saved") {
                    Text("You confirmed it, so this phone no longer keeps a copy. Keep what you wrote down somewhere safe.")
                }
                Spacer(minLength: 0)
            } else if locked {
                LumiaEmptyState(icon: "lock", title: "Unlock to see your phrase", action: ("Show the phrase", { Task { await unlock() } })) {
                    Text("Your recovery phrase is shown only after this phone's passcode, Face ID or Touch ID.")
                }
                Spacer(minLength: 0)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        // Once: back from the check the words are still here.
        .task { if words == nil, !locked, !saved { await unlock() } }
        .onChange(of: scenePhase) { _, phase in
            // Gone to the background: nothing of the words is kept, and the
            // next look asks for the unlock again.
            if phase == .background, words != nil {
                words = nil
                locked = true
            }
        }
        .sheet(isPresented: $warning, onDismiss: {
            if exportAfterWarning { exportAfterWarning = false; exporting = true }
        }) {
            SaveWarningSheet {
                exportAfterWarning = true
                warning = false
            } cancel: {
                warning = false
            }
        }
        .fileExporter(isPresented: $exporting, document: PhraseDocument(text: PhraseWords.file(words ?? [])), contentType: .plainText, defaultFilename: PhraseWords.fileName) { result in
            switch result {
            case .success: note = "Saved. Keep the file somewhere only you can open."
            case let .failure(error): note = "Not saved: \(error.localizedDescription)"
            }
        }
    }

    /// Two columns, 1 to 12 down the left and 13 to 24 down the right; dots
    /// in place of the words while the app is not in front (the app
    /// switcher's picture is taken then).
    private func wordGrid(_ words: [String]) -> some View {
        let shown = scenePhase == .active
        return HStack(alignment: .top, spacing: LumiaTokens.space6) {
            ForEach(Array(PhraseWords.columns(words, 2).enumerated()), id: \.offset) { _, column in
                VStack(spacing: LumiaTokens.space6) {
                    ForEach(column, id: \.number) { cell in
                        HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space8) {
                            Text("\(cell.number)").font(LumiaTokens.textMeta.font).fontWeight(.semibold)
                                .foregroundStyle(LumiaTokens.inkMuted.color)
                                .frame(width: 20, alignment: .trailing)
                            Text(shown ? cell.word : "•••••")
                                .font(.system(size: LumiaTokens.textBody.size, weight: .medium, design: .monospaced))
                                .foregroundStyle(LumiaTokens.ink.color)
                            Spacer(minLength: 0)
                        }
                        .padding(.vertical, 7).padding(.horizontal, LumiaTokens.space10)
                        .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius10))
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(shown ? "Word \(cell.number): \(cell.word)" : "Word \(cell.number), hidden")
                    }
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Your 24 words, in order")
    }

    /// The phone's own unlock, then the words; a phone with no passcode set
    /// offers none, and the words are shown.
    private func unlock() async {
        locked = false
        let context = LAContext()
        var unavailable: NSError?
        #if DEBUG
        // A smoke run (scripts/smoke-private-messages.sh) cannot answer the
        // system's passcode prompt, which a simulator shows too.
        let asks = ProcessInfo.processInfo.environment["LUMIA_DEV_NO_UNLOCK"] != "1"
        #else
        let asks = true
        #endif
        if asks, context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &unavailable) {
            do {
                _ = try await context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: "Show your recovery phrase")
            } catch {
                locked = true
                return
            }
        }
        do {
            if let found = try await session.phrase() {
                words = found
            } else {
                saved = true
            }
        } catch {
            note = error.lumiaMessage
            locked = true
        }
    }

    /// The system's print sheet with a plain page: the warning and the numbered words.
    private func print(_ words: [String]) {
        let info = UIPrintInfo.printInfo()
        info.outputType = .general
        info.jobName = PhraseWords.fileName
        let controller = UIPrintInteractionController.shared
        controller.printInfo = info
        controller.printFormatter = UIMarkupTextPrintFormatter(markupText: PhraseWords.page(words))
        controller.present(animated: true)
    }
}

/// What "Save as a file" asks first; only "Save anyway" writes it.
private struct SaveWarningSheet: View {
    let save: () -> Void
    let cancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space18) {
            VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                Text(PhraseWords.saveTitle).font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
                    .accessibilityAddTraits(.isHeader)
                Text(PhraseWords.saveLine).font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    .fixedSize(horizontal: false, vertical: true)
            }
            LumiaNotice(icon: "exclamationmark.triangle", tone: .warning) { Text(PhraseWords.saveNote) }
            VStack(spacing: LumiaTokens.space8) {
                Button(action: save) { Text("Save anyway").frame(maxWidth: .infinity) }.buttonStyle(.lumiaPrimary)
                Button(action: cancel) { Text("Write it down instead").frame(maxWidth: .infinity) }.buttonStyle(.lumiaSecondary)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, LumiaTokens.space24)
        .padding(.top, LumiaTokens.space24)
        .presentationDetents([.medium])
        .presentationDragIndicator(.visible)
        .presentationBackground(LumiaTokens.surface.color)
    }
}

/// Step 2 of 2: three of the words, at the positions the core chose, each
/// field offering the wordlist's words as it is typed. Right ends the
/// reminders; wrong shows the phrase again, and the next check asks others.
struct PhraseCheckScreen: View {
    let session: LumiaSession
    /// A word was wrong: back to the phrase, saying so.
    let wrong: () -> Void
    /// Back to the phrase, nothing said.
    let back: () -> Void
    let done: () -> Void
    @State private var check: PhraseCheck?
    @FocusState private var focus: Int?
    @State private var busy = false
    @State private var error = ""

    var body: some View {
        PanelScreen(back: "Recovery phrase", title: "Check your phrase", subtitle: "Step 2 of 2") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space14) {
                    Text("Type these three words from what you wrote down, to make sure it's right.")
                        .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkBody.color)
                        .fixedSize(horizontal: false, vertical: true)
                    if let check {
                        ForEach(Array(check.positions.enumerated()), id: \.offset) { i, position in
                            field(i, position: position)
                        }
                    } else if error.isEmpty {
                        ProgressView().frame(maxWidth: .infinity)
                    }
                    if !error.isEmpty { LumiaErrorText(error) }
                }
                .padding(LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
            PanelFooter {
                Button { Task { await confirm() } } label: { Text("Confirm").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary).disabled(busy || !(check?.canConfirm() ?? false))
                Button(action: back) { Text("Show the phrase again").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaSecondary)
            }
        }
        .task {
            do {
                check = PhraseCheck(positions: try await session.phraseQuiz())
                focus = 0
            } catch {
                self.error = error.lumiaMessage
            }
        }
    }

    private func field(_ i: Int, position: UInt32) -> some View {
        let text = Binding(get: { check?.answers[i] ?? "" }, set: { check?.answers[i] = $0 })
        let suggestions = focus == i ? PhraseCheck.suggestions(text.wrappedValue) : []
        return VStack(alignment: .leading, spacing: LumiaTokens.space4) {
            LumiaField("Word \(position)", text: text, placeholder: "", spoken: "Word \(position)")
                .textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.asciiCapable)
                .focused($focus, equals: i)
                .submitLabel(i + 1 < (check?.positions.count ?? 0) ? .next : .done)
                .onSubmit { focus = i + 1 < (check?.positions.count ?? 0) ? i + 1 : nil }
            if !suggestions.isEmpty {
                VStack(spacing: 0) {
                    ForEach(suggestions, id: \.self) { word in
                        Button {
                            check?.answers[i] = word
                            focus = i + 1 < (check?.positions.count ?? 0) ? i + 1 : nil
                        } label: {
                            Text(word).font(.system(size: LumiaTokens.textBody.size, design: .monospaced))
                                .foregroundStyle(LumiaTokens.ink.color)
                                .frame(maxWidth: .infinity, minHeight: LumiaTokens.controlHeight, alignment: .leading)
                                .padding(.horizontal, LumiaTokens.space14)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(LumiaTokens.space4)
                .background(LumiaTokens.raised.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.border.color))
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Words starting with \(PhraseCheck.normal(text.wrappedValue))")
            }
        }
    }

    private func confirm() async {
        guard let check else { return }
        busy = true
        error = ""
        defer { busy = false }
        do {
            if try await session.confirmPhrase(check.words) { done() } else { wrong() }
        } catch {
            self.error = error.lumiaMessage
        }
    }
}

// MARK: Restore

/// Restore with the recovery phrase (identity spec 4.3): the 24 words in
/// one field, counted, Restore at 24. This phone becomes the identity's
/// only device and signs in by it, from inside the app or from sign-in.
struct RestoreScreen: View {
    let session: LumiaSession
    var back = "Home"
    /// Where the back button goes when this is not pushed (from sign-in).
    var close: (() -> Void)?
    let restored: () -> Void
    @State private var text = ""
    @State private var busy = false
    @State private var error = ""

    var body: some View {
        PanelScreen(back: back, title: "Restore", subtitle: "With your recovery phrase", close: close) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space14) {
                    VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                        HStack {
                            Text("Your 24 words, in order").font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                            Spacer()
                            Text(RestoreWords.countLabel(text)).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                                .accessibilityHidden(true)
                        }
                        TextEditor(text: $text)
                            .font(.system(size: LumiaTokens.textBody.size, design: .monospaced))
                            .foregroundStyle(LumiaTokens.inkField.color)
                            .textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.asciiCapable)
                            .scrollContentBackground(.hidden)
                            .frame(minHeight: 150)
                            .padding(LumiaTokens.space6)
                            .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.borderForm.color))
                            .accessibilityLabel("Your 24 words, in order")
                            .accessibilityValue(RestoreWords.countLabel(text))
                    }
                    LumiaNotice(icon: "iphone") {
                        Text("Restoring makes this phone your identity's only device. Every other device of yours stops reading private messages, so a lost one can't keep reading them.")
                    }
                    LumiaNotice(icon: "clock.arrow.circlepath") {
                        Text("Messages from before can't be read here unless history backup was on.")
                    }
                    if !error.isEmpty { LumiaErrorText(error) }
                }
                .padding(LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
            PanelFooter {
                Button { Task { await restore() } } label: { Text("Restore").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary).disabled(busy || !RestoreWords.canRestore(text))
                if !RestoreWords.canRestore(text) {
                    Text("Restore turns on once all 24 words are in.")
                        .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                }
            }
        }
    }

    private func restore() async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            try await session.restore(phrase: RestoreWords.phrase(text))
            text = ""
            restored()
        } catch {
            self.error = "Not restored: \(error.lumiaMessage)"
        }
    }
}

// MARK: Start over

/// Start over with a new identity (identity spec 4.7), for someone with
/// neither a device of theirs nor the phrase: what stays and what does
/// not, who will see the key change, the password (or another way to
/// confirm it's them), and the danger's button.
struct StartOverScreen: View {
    @Environment(\.dismiss) private var dismiss
    let session: LumiaSession
    var back = "Home"
    let done: () -> Void
    @State private var way = ReauthWay.password
    @State private var secret = ""
    @State private var error = ""
    @State private var busy = false
    @State private var provider = ""

    private var people: Int { StartOverWords.people(in: session.snapshot?.channels ?? [], me: session.me) }

    var body: some View {
        PanelScreen(back: back, title: "Start over", subtitle: "With a new identity") {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space14) {
                    Text("Without a device or your recovery phrase, your old identity can't come back. You can start over on this phone with a new one.")
                        .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkBody.color)
                        .fixedSize(horizontal: false, vertical: true)
                    VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                        ForEach([
                            "Your account, servers and channels stay as they are.",
                            "Private messages from before stay unreadable.",
                            StartOverWords.keyChange(people: people),
                            "You'll get a new recovery phrase to save.",
                        ], id: \.self) { line in
                            HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space10) {
                                Circle().fill(LumiaTokens.inkMuted.color).frame(width: 6, height: 6).alignmentGuide(.firstTextBaseline) { $0[.bottom] + 2 }
                                    .accessibilityHidden(true)
                                Text(line).font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                    LumiaNotice(icon: "exclamationmark.triangle", tone: .warning) {
                        Text("Only do this if the old phrase is truly gone. If you find it later, your old identity can be restored, but not onto this account.")
                    }
                    ReauthFields(session: session, label: "Confirm with your password", way: $way, secret: $secret, error: $error, provider: provider) {
                        Task { await run(viaProvider: true) }
                    }
                }
                .padding(LumiaTokens.space16)
            }
            .scrollDismissesKeyboard(.interactively)
            PanelFooter {
                Button { Task { await run(viaProvider: false) } } label: { Text("Start over with a new identity").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaDanger).disabled(busy || !canTry(way, secret))
                Button { dismiss() } label: { Text("Keep looking for the phrase").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaSecondary)
            }
        }
        .task { provider = (try? await session.instanceInfo())?.oidcName ?? "" }
    }

    private func run(viaProvider: Bool) async {
        busy = true
        error = ""
        defer { busy = false }
        do {
            if viaProvider { try await session.reauthThroughBrowser() } else { try await reauth(session, way: way, secret: secret) }
        } catch BrowserSheetError.cancelled {
            return
        } catch {
            self.error = reauthRefusal(error, way: way)
            return
        }
        do {
            try await session.startOver()
            // A phrase put off before belongs to the identity just left.
            var reminder = PhraseReminder()
            reminder.clear()
            done()
        } catch {
            self.error = error.lumiaCode == "ALREADY_EXISTS" ? "This phone's identity is already another account's here." : error.lumiaMessage
        }
    }
}
