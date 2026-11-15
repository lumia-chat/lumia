// Approving another device from this phone, which holds the identity
// (identity spec 4.2 steps 2 to 6): the request the new device shows,
// pasted; what it would add, read by the core before anything is sent,
// with the warning that it will read private messages; Approve behind the
// phone's own unlock; then the code, large and in fours for reading out.
// The new device's half is `LinkView`. The rules and words are LumiaKit's
// (`Linking.swift`).
import LumiaFFI
import LumiaKit
import SwiftUI

struct ApproveDeviceScreen: View {
    let session: LumiaSession
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var busy = false
    @State private var error = ""
    @FocusState private var typing: Bool
    /// The device approved and the code it claims with.
    @State private var approved: (summary: LinkRequestSummary, code: String)?

    var body: some View {
        PanelScreen(back: "Devices", title: "Approve a device", subtitle: approved == nil ? "Add a device to your identity" : "Approved") {
            if let approved {
                code(approved.code, for: approved.summary)
            } else {
                paste
            }
        }
        // Whether the request's device is already one of these is read from it.
        .task { await session.refreshRoster() }
    }

    // MARK: The request

    @ViewBuilder private var paste: some View {
        let reading = LinkApproval.read(text)
        ScrollView {
            VStack(alignment: .leading, spacing: LumiaTokens.space14) {
                VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                    Text("Paste the request the new device shows").font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        .accessibilityHidden(true)
                    TextEditor(text: $text)
                        .font(.system(size: LumiaTokens.textMeta.size, design: .monospaced))
                        .foregroundStyle(LumiaTokens.inkField.color)
                        .textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.asciiCapable)
                        .scrollContentBackground(.hidden)
                        .frame(minHeight: 120)
                        .padding(LumiaTokens.space6)
                        .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.borderForm.color))
                        .accessibilityLabel("Link request")
                        .focused($typing)
                    HStack(spacing: LumiaTokens.space8) {
                        // The system's own Paste reads the clipboard without
                        // asking each time, which reading it from a button
                        // of ours would.
                        // The keyboard goes, so what the request would add shows.
                        PasteButton(payloadType: String.self) { strings in
                            Task { @MainActor in
                                text = strings.first ?? ""
                                error = ""
                                typing = false
                            }
                        }
                        .labelStyle(.titleAndIcon)
                        .buttonBorderShape(.roundedRectangle(radius: LumiaTokens.radius8))
                        .tint(LumiaTokens.accent.color)
                        if !text.isEmpty {
                            Button("Clear") { text = ""; error = "" }.buttonStyle(.lumiaSecondary)
                        }
                    }
                    Text("On the new device, start linking, then copy or share the request it shows to this phone.")
                        .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkQuiet.color)
                        .fixedSize(horizontal: false, vertical: true)
                }
                switch reading {
                case .empty:
                    EmptyView()
                case .notARequest:
                    LumiaErrorText(LinkApproval.notARequest)
                case let .request(summary, _):
                    adds(summary)
                }
                if !error.isEmpty { LumiaErrorText(error) }
            }
            .padding(LumiaTokens.space16)
        }
        .scrollDismissesKeyboard(.interactively)
        PanelFooter {
            Button {
                if case let .request(summary, request) = reading { Task { await approve(request, summary) } }
            } label: {
                Text("Approve").frame(maxWidth: .infinity)
            }
            .buttonStyle(.lumiaPrimary)
            .disabled(busy || !approvable(reading))
            Text("Approving asks for this phone's passcode, Face ID or Touch ID.")
                .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
        }
    }

    /// What the request would add, and what that means; or why this phone
    /// will not approve it.
    private func adds(_ summary: LinkRequestSummary) -> some View {
        let refusal = LinkApproval.refusal(of: summary, among: session.roster?.devices ?? [])
        return VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            HStack(spacing: LumiaTokens.space10) {
                Image(systemName: summary.kind == "web" ? "globe" : "laptopcomputer.and.iphone")
                    .foregroundStyle(LumiaTokens.iconStrong.color)
                    .accessibilityHidden(true)
                Text(LinkApproval.adds(summary)).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.ink.color)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let refusal {
                LumiaErrorText(refusal)
            } else {
                LumiaNotice(icon: "exclamationmark.triangle", tone: .warning) {
                    Text("\(LinkApproval.warning) Approve it only if it's yours and you just started linking on it.")
                }
            }
        }
        .padding(LumiaTokens.space12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.line.color))
        .accessibilityElement(children: .combine)
    }

    private func approvable(_ reading: LinkApproval.Reading) -> Bool {
        guard case let .request(summary, _) = reading else { return false }
        return LinkApproval.refusal(of: summary, among: session.roster?.devices ?? []) == nil
    }

    /// The phone's unlock, then the core publishes the roster with the
    /// device in it and uploads the sealed bundle; a cancelled unlock sends
    /// nothing and says nothing.
    private func approve(_ request: String, _ summary: LinkRequestSummary) async {
        error = ""
        guard await ownerUnlock("Approve \(LinkApproval.adds(summary))") else { return }
        busy = true
        defer { busy = false }
        do {
            let code = try await session.identityApprove(request: request)
            text = ""
            approved = (summary, code)
            AccessibilityNotification.Announcement("Approved. The code is \(LinkApproval.spaced(code))").post()
        } catch {
            self.error = LinkApproval.refusal(error)
        }
    }

    // MARK: The code

    private func code(_ code: String, for summary: LinkRequestSummary) -> some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(spacing: LumiaTokens.space16) {
                    Text("\(LinkApproval.adds(summary)) is approved.")
                        .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        .multilineTextAlignment(.center)
                    Text(LinkApproval.spaced(code))
                        .font(.system(size: LumiaTokens.titleAuth.size + LumiaTokens.space10, weight: .bold, design: .monospaced))
                        .tracking(LumiaTokens.space4)
                        .foregroundStyle(LumiaTokens.ink.color)
                        .textSelection(.enabled)
                        .padding(.vertical, LumiaTokens.space16).padding(.horizontal, LumiaTokens.space16)
                        .frame(maxWidth: .infinity)
                        .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel("Code")
                        .accessibilityValue(Text(code).speechSpellsOutCharacters())
                    Text(LinkApproval.codeNote)
                        .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.ink.color)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(LumiaTokens.space16)
                .padding(.top, LumiaTokens.space24)
            }
            PanelFooter {
                Button { dismiss() } label: { Text("Done").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary)
            }
        }
    }
}
