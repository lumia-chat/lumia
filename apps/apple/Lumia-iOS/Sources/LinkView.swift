// Link this phone as a device of an identity. Signed in to the account,
// by a prompt (identity spec 4.2.1): every device that holds the identity
// asks about this phone, the person checks that both show the same six
// digits and approves there, and this phone claims and signs in by itself.
// "Link another way" (and a phone not signed in, which cannot open a link
// session) is 4.2's: show the request for a device that holds the identity
// to approve, then claim the code it gives. The rules and words are
// LumiaKit's (`Linking.swift`).
import LumiaKit
import SwiftUI

struct LinkView: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    /// The copied request and typed code, chosen under the prompt's screen.
    @State private var anotherWay = false

    var body: some View {
        ScrollView {
            VStack(spacing: LumiaTokens.space16) {
                LumiaCard(title: "Link this phone", subtitle: "A device you already use approves this one.", width: nil) {
                    if session.signedIn && !anotherWay {
                        LinkByPromptSteps(stage: session.linking, error: startError.isEmpty ? session.linkingError : startError, start: start, cancel: cancel) {
                            anotherWay = true
                            Task { await session.linkCancel() }
                        }
                    } else {
                        LinkByRequest(session: session)
                    }
                }
            }
            .padding(LumiaTokens.space16)
            .padding(.top, LumiaTokens.space40)
        }
        .scrollDismissesKeyboard(.interactively)
        // "Link from a device you use" brought the person here: it starts at once.
        .task {
            if session.signedIn, session.linking == nil { await start() }
        }
        // Asked when the screen opens and every two seconds while it is
        // under way, beside every account event the session hears.
        .task(id: !anotherWay && session.linking?.polls == true) {
            while !Task.isCancelled, !anotherWay, session.linking?.polls == true {
                await session.linkStep()
                try? await Task.sleep(for: LinkByPrompt.pollEvery)
            }
        }
        .onChange(of: session.linking) { _, stage in
            switch stage {
            case .linked:
                // As after a claimed code: into the app, private messages ready.
                session.linkDone()
                store.route = .app
            case let .compare(digits):
                AccessibilityNotification.Announcement("Your other device should show \(digits)").post()
            default:
                break
            }
        }
    }

    /// Why linking could not start, said where the spinner would be.
    @State private var startError = ""

    private func start() async {
        startError = ""
        do {
            try await session.linkStart()
        } catch {
            startError = LinkByPrompt.startRefusal(error)
        }
    }

    private func cancel() {
        Task {
            await session.linkCancel()
            store.route = .app
        }
    }
}

/// The prompt's steps on this phone: waiting with a quiet spinner, the
/// digits to compare, or how it ended with Try again; Cancel throughout.
/// Drawn from values, so a debug build shows each without an instance.
struct LinkByPromptSteps: View {
    let stage: LinkByPrompt.Stage?
    let error: String?
    let start: () async -> Void
    let cancel: () -> Void
    let anotherWay: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space12) {
            switch stage {
            case nil where error?.isEmpty == false:
                // It could not start.
                Button { Task { await start() } } label: { Text("Try again").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary)
            case nil, .linked?:
                HStack { Spacer(); ProgressView(); Spacer() }
                    .frame(minHeight: LumiaTokens.controlHeight)
            case .waiting?:
                line(LinkByPrompt.words(.waiting))
                HStack(spacing: LumiaTokens.space8) {
                    ProgressView().controlSize(.small)
                    Text("Waiting for your other device").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                }
                .accessibilityElement(children: .combine)
            case let .compare(digits)?:
                Text(digits)
                    .font(.system(size: LumiaTokens.titleAuth.size + LumiaTokens.space10, weight: .bold, design: .monospaced))
                    .tracking(LumiaTokens.space4)
                    .foregroundStyle(LumiaTokens.ink.color)
                    .padding(LumiaTokens.space16)
                    .frame(maxWidth: .infinity)
                    .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("Number")
                    .accessibilityValue(Text(digits).speechSpellsOutCharacters())
                line(LinkByPrompt.words(.compare(digits)))
            case let ended?:
                LumiaNotice(icon: "exclamationmark.triangle", tone: .warning) { Text(LinkByPrompt.words(ended)) }
                Button { Task { await start() } } label: { Text("Try again").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary)
            }
            if let error, !error.isEmpty { LumiaErrorText(error) }
            Button(action: cancel) { Text("Cancel").frame(maxWidth: .infinity) }
                .buttonStyle(.lumiaSecondary)
            if stage == .waiting {
                Button("Link another way", action: anotherWay)
                    .buttonStyle(.lumiaLink)
                    .frame(maxWidth: .infinity)
                    .frame(minHeight: LumiaTokens.controlHeight)
            }
        }
    }

    private func line(_ text: String) -> some View {
        Text(text)
            .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// 4.2 by hand: the request to copy or share to a device that holds the
/// identity, then the code it gives typed back, which claims and signs in.
private struct LinkByRequest: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    @State private var request = ""
    @State private var code = ""
    @State private var error = ""
    @State private var busy = false
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space12) {
            if request.isEmpty {
                Text("This phone gets its own device key.")
                    .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                Button { Task { await start() } } label: { Text("Start linking").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary)
            } else {
                Text(LinkApproval.instruction)
                    .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                Text(request)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(LumiaTokens.inkField.color)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(LumiaTokens.space10)
                    .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                HStack(spacing: LumiaTokens.space8) {
                    Button(copied ? "Copied" : "Copy request") {
                        UIPasteboard.general.string = request
                        copied = true
                    }
                    .buttonStyle(.lumiaSecondary)
                    ShareLink(item: request) { Label("Share", systemImage: "square.and.arrow.up") }
                        .buttonStyle(.lumiaSecondary)
                }
                LumiaField("Code from the approving device", text: $code, placeholder: "8 characters")
                    .textInputAutocapitalization(.characters).autocorrectionDisabled()
                    .onSubmit { Task { await claim() } }
                Button { Task { await claim() } } label: { Text("Claim and sign in").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary)
                    .disabled(busy || LinkApproval.typedCode(code).isEmpty)
            }
            if !error.isEmpty { LumiaErrorText(error) }
            Button(session.signedIn ? "Back" : "Back to sign in") { store.route = session.signedIn ? .app : .signIn }
                .buttonStyle(.lumiaLink).frame(maxWidth: .infinity)
        }
    }

    private func start() async {
        error = ""
        do { request = try await session.identityLinkRequest(label: LumiaSession.deviceLabel) } catch { self.error = error.lumiaMessage }
    }

    private func claim() async {
        error = ""
        busy = true
        defer { busy = false }
        do {
            try await session.identityClaim(code: LinkApproval.typedCode(code))
            store.route = .app
        } catch {
            self.error = error.lumiaMessage
        }
    }
}
