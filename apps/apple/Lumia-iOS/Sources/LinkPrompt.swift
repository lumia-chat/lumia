// Another device asks to be linked (identity spec 4.2.1), on a phone that
// holds the identity: a sheet over whatever is on screen, one prompt at a
// time, oldest first. Continue answers it; both devices then show the same
// six digits, and "They match" approves behind the phone's own unlock (as
// Approve a device does); "Not me" and "They don't match" decline it. The
// new device's half is `LinkView`; the rules and words are LumiaKit's
// (`LinkByPrompt` in `Linking.swift`), and the prompts are the session's.
import LumiaFFI
import LumiaKit
import SwiftUI

/// The sheet's content and what its buttons do, over the session.
struct LinkPromptSheet: View {
    let session: LumiaSession
    @State private var busy = false
    @State private var error = ""

    var body: some View {
        if let sheet = session.linkSheet {
            LinkPromptCard(sheet: sheet, busy: busy, error: error, act: act)
                // A different prompt, or the next stage, starts with nothing said.
                .onChange(of: sheet) { _, now in
                    error = ""
                    if case let .prompt(p) = now, case let .compare(digits) = LinkByPrompt.PromptStage(p) {
                        AccessibilityNotification.Announcement("\(LinkByPrompt.title(p)) \(digits)").post()
                    }
                }
        }
    }

    private func act(_ action: LinkPromptCard.Action, _ prompt: LinkPrompt) {
        Task {
            error = ""
            if action == .match {
                guard await ownerUnlock("Link \(LinkByPrompt.name(prompt.request))") else { return }
            }
            busy = true
            defer { busy = false }
            do {
                switch action {
                case .answer: try await session.linkPromptAnswer(prompt)
                case .match: try await session.linkPromptConfirm(prompt)
                case .notMe: try await session.linkPromptDecline(prompt)
                case .noMatch: try await session.linkPromptDecline(prompt, mismatch: true)
                }
            } catch {
                self.error = LinkByPrompt.refusal(error)
            }
        }
    }
}

/// A prompt at one of its stages, or how it ended; drawn from values, so a
/// debug build shows each without an instance.
struct LinkPromptCard: View {
    enum Action { case answer, match, notMe, noMatch }

    let sheet: LinkByPrompt.Sheet
    var busy = false
    var error = ""
    let act: (Action, LinkPrompt) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space18) {
            switch sheet {
            case let .prompt(prompt):
                asked(prompt)
            case let .linked(words):
                ended(icon: "checkmark.circle", tone: LumiaTokens.online.color, words)
            case .stopped:
                ended(icon: "xmark.circle", tone: LumiaTokens.inkSecondary.color, LinkByPrompt.stopped)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, LumiaTokens.space24)
        .padding(.top, LumiaTokens.space24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .presentationDetents([.medium])
        .presentationBackground(LumiaTokens.surface.color)
        // Answered by its buttons: swiped away it would only come back.
        .interactiveDismissDisabled()
    }

    @ViewBuilder private func asked(_ prompt: LinkPrompt) -> some View {
        let stage = LinkByPrompt.PromptStage(prompt)
        VStack(alignment: .leading, spacing: LumiaTokens.space6) {
            HStack(alignment: .firstTextBaseline, spacing: LumiaTokens.space10) {
                Image(systemName: stage == .refused ? "exclamationmark.triangle" : prompt.request.kind == "web" ? "globe" : "laptopcomputer.and.iphone")
                    .foregroundStyle(stage == .refused ? LumiaTokens.danger.color : LumiaTokens.iconStrong.color)
                    .accessibilityHidden(true)
                Text(LinkByPrompt.title(prompt)).font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityAddTraits(.isHeader)
            }
            if stage == .new {
                Text(LinkByPrompt.onlyIfYours)
                    .font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        if case let .compare(digits) = stage {
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
        }
        if stage == .waiting {
            HStack(spacing: LumiaTokens.space8) {
                ProgressView().controlSize(.small)
                Text("Waiting for it to show a number").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
            }
            .accessibilityElement(children: .combine)
        }
        if !error.isEmpty { LumiaErrorText(error) }
        VStack(spacing: LumiaTokens.space8) {
            switch stage {
            case .new:
                button("Continue", .lumiaPrimary) { act(.answer, prompt) }
                button("Not me", .lumiaSecondary) { act(.notMe, prompt) }
            case .compare:
                button("They match", .lumiaPrimary) { act(.match, prompt) }
                button("They don't match", .lumiaSecondary) { act(.noMatch, prompt) }
                Text("Approving asks for this phone's passcode, Face ID or Touch ID.")
                    .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    .frame(maxWidth: .infinity, alignment: .leading)
            case .waiting, .refused:
                button("Not me", .lumiaSecondary) { act(.notMe, prompt) }
            }
        }
        .disabled(busy)
    }

    private func button<S: ButtonStyle>(_ title: String, _ style: S, run: @escaping () -> Void) -> some View {
        Button(action: run) { Text(title).frame(maxWidth: .infinity) }.buttonStyle(style)
    }

    private func ended(icon: String, tone: Color, _ words: String) -> some View {
        HStack(spacing: LumiaTokens.space10) {
            Image(systemName: icon).foregroundStyle(tone).accessibilityHidden(true)
            Text(words).font(LumiaTokens.titleAuth.font).foregroundStyle(LumiaTokens.ink.color)
                .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
        .onAppear { AccessibilityNotification.Announcement(words).post() }
    }
}
