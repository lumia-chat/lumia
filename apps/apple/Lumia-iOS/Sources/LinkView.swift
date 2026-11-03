// Link this phone as a device of an identity (identity spec 4.2): show
// the request for a device that holds the identity to approve, then
// claim the code it gives and sign in by identity proof.
import LumiaKit
import SwiftUI

struct LinkView: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    @State private var request = ""
    @State private var code = ""
    @State private var error = ""
    @State private var busy = false
    @State private var copied = false

    var body: some View {
        ScrollView {
            VStack(spacing: LumiaTokens.space16) {
                LumiaCard(title: "Link this phone", subtitle: "A device you already use approves this one.", width: nil) {
                    VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                        if request.isEmpty {
                            Text("This phone gets its own device key.")
                                .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                            Button { Task { await start() } } label: { Text("Start linking").frame(maxWidth: .infinity) }
                                .buttonStyle(.lumiaPrimary)
                        } else {
                            Text("On a device that holds your identity, approve this request and enter the code it gives you.")
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
                                .disabled(busy || code.trimmingCharacters(in: .whitespaces).isEmpty)
                        }
                        if !error.isEmpty { LumiaErrorText(error) }
                        Button("Back to sign in") { store.route = session.signedIn ? .app : .signIn }
                            .buttonStyle(.lumiaLink).frame(maxWidth: .infinity)
                    }
                }
            }
            .padding(LumiaTokens.space16)
            .padding(.top, LumiaTokens.space40)
        }
        .scrollDismissesKeyboard(.interactively)
    }

    private func start() async {
        error = ""
        do { request = try await session.identityLinkRequest(label: "phone") } catch { self.error = error.lumiaMessage }
    }

    private func claim() async {
        error = ""
        busy = true
        defer { busy = false }
        do {
            try await session.identityClaim(code: code.trimmingCharacters(in: .whitespaces))
            store.route = .app
        } catch {
            self.error = error.lumiaMessage
        }
    }
}
