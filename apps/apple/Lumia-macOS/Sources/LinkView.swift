// Link this computer as a device of an identity (identity spec 4.2):
// show the request for a device that holds the identity to approve, then
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

    var body: some View {
        LumiaCard(title: "Link this computer", subtitle: "A device you already use approves this one.") {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                if request.isEmpty {
                    Text("This computer gets its own device key.")
                        .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    Button("Start linking") { Task { await start() } }
                        .buttonStyle(.lumiaPrimary).frame(maxWidth: .infinity)
                } else {
                    Text("On a device that holds your identity, approve this request and enter the code it gives you.")
                        .font(LumiaTokens.textBody.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    TextEditor(text: .constant(request))
                        .font(.system(.caption, design: .monospaced))
                        .frame(height: 96)
                        .scrollContentBackground(.hidden)
                        .padding(LumiaTokens.space6)
                        .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                    Button("Copy request") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(request, forType: .string)
                    }
                    .buttonStyle(.lumiaSecondary)
                    LumiaField("Code from the approving device", text: $code, placeholder: "8 characters")
                        .onSubmit { Task { await claim() } }
                    Button("Claim and sign in") { Task { await claim() } }
                        .buttonStyle(.lumiaPrimary)
                        .disabled(busy || code.trimmingCharacters(in: .whitespaces).isEmpty)
                        .frame(maxWidth: .infinity)
                        .keyboardShortcut(.defaultAction)
                }
                if !error.isEmpty { LumiaErrorText(error) }
                Button("Back to sign in") { store.route = .signIn }
                    .buttonStyle(.lumiaLink).frame(maxWidth: .infinity)
            }
        }
    }

    private func start() async {
        error = ""
        do { request = try await session.identityLinkRequest(label: "desktop") } catch { self.error = error.lumiaMessage }
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
