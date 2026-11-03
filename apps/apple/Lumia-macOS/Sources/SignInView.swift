// Sign in: the instance (with its trust card, architecture 6.1), then a
// password with a second factor when the account asks for one, or a new
// account; the same flow as the web's sign-in screen.
import LumiaFFI
import LumiaKit
import SwiftUI

struct SignInView: View {
    @Environment(AppStore.self) private var store
    @State private var instance = ""
    @State private var info: InstanceSummary?
    @State private var reaching = false
    @State private var creating = false
    @State private var handle = ""
    @State private var password = ""
    @State private var totp = ""
    @State private var needsTotp = false
    @State private var error = ""
    @State private var busy = false

    var body: some View {
        VStack(spacing: LumiaTokens.space24) {
            Image("Logo").resizable().scaledToFit().frame(height: 48)
            if let info {
                accountCard(info)
            } else {
                instanceCard
            }
        }
        .padding(LumiaTokens.space40)
        .task {
            if let saved = store.savedInstance { instance = saved }
            if store.session != nil { await reach() }
        }
    }

    private var instanceCard: some View {
        LumiaCard(title: "Welcome", subtitle: "Which instance do you want to sign in to?") {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                LumiaField("Instance", text: $instance, placeholder: "https://chat.example.com")
                    .onSubmit { Task { await connect() } }
                if !error.isEmpty { LumiaErrorText(error) }
                if let openError = store.openError, error.isEmpty { LumiaErrorText(openError) }
                if reaching {
                    Text("Reaching the instance…").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                }
                Button("Continue") { Task { await connect() } }
                    .buttonStyle(.lumiaPrimary)
                    .disabled(reaching)
                    .frame(maxWidth: .infinity)
            }
        }
    }

    private func accountCard(_ info: InstanceSummary) -> some View {
        LumiaCard(title: creating ? "Create an account" : "Welcome back", subtitle: creating ? "A new account on \(info.host)." : "Sign in to pick up your conversations.") {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    Text(info.host).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color)
                    Text("run by \(info.operatorName.isEmpty ? "an unnamed operator" : info.operatorName)\(info.softwareVersion.isEmpty ? "" : " · Lumia \(info.softwareVersion)")")
                        .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                    Button("Change instance") { self.info = nil; error = "" }.buttonStyle(.lumiaLink)
                }
                .padding(LumiaTokens.space10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))

                LumiaField(creating ? "Handle" : "Email or handle", text: $handle, placeholder: creating ? "yourname" : "you@example.com")
                LumiaField("Password", text: $password, placeholder: creating ? "At least 12 characters" : "Your password", secure: true)
                    .onSubmit { Task { await submit() } }
                if needsTotp {
                    LumiaField("Verification code", text: $totp, placeholder: "6 digits from your authenticator")
                        .onSubmit { Task { await submit() } }
                }
                if !error.isEmpty { LumiaErrorText(error) }
                Button(creating ? "Create account" : "Sign in") { Task { await submit() } }
                    .buttonStyle(.lumiaPrimary)
                    .disabled(busy)
                    .frame(maxWidth: .infinity)
                    .keyboardShortcut(.defaultAction)
                if !creating, info.identityProofSignInEnabled {
                    Button {
                        store.route = .link
                    } label: {
                        Label("Link this computer to your identity", systemImage: "key")
                    }
                    .buttonStyle(.lumiaSecondary)
                    .frame(maxWidth: .infinity)
                }
                HStack(spacing: LumiaTokens.space4) {
                    if creating {
                        Text("Already have an account?")
                        Button("Sign in") { creating = false; error = "" }.buttonStyle(.lumiaLink)
                    } else {
                        Text("New to Lumia?")
                        if info.openRegistration {
                            Button("Create an account") { creating = true; error = "" }.buttonStyle(.lumiaLink)
                        } else {
                            Text("This instance is not open for new accounts.")
                        }
                    }
                }
                .font(LumiaTokens.textMeta.font)
                .foregroundStyle(LumiaTokens.inkSecondary.color)
                .frame(maxWidth: .infinity)
            }
        }
    }

    private func connect() async {
        error = ""
        var url = instance.trimmingCharacters(in: .whitespacesAndNewlines)
        while url.hasSuffix("/") { url.removeLast() }
        guard url.hasPrefix("http://") || url.hasPrefix("https://"), let parsed = URL(string: url) else {
            error = "Enter the instance address, starting with http:// or https://"
            return
        }
        do {
            try await store.open(parsed)
            await reach()
        } catch {
            self.error = error.lumiaMessage
        }
    }

    private func reach() async {
        guard let session = store.session else { return }
        reaching = true
        defer { reaching = false }
        do {
            info = try await session.instanceInfo()
        } catch {
            self.error = "Could not reach that instance: \(error.lumiaMessage)"
        }
    }

    private func submit() async {
        guard let session = store.session else { return }
        error = ""
        busy = true
        defer { busy = false }
        do {
            if creating {
                try await session.createAccount(handle: handle, password: password)
            } else {
                try await session.signIn(handle: handle, password: password, totpCode: needsTotp ? totp : nil)
            }
            store.route = .app
        } catch {
            switch error.lumiaCode {
            case "FAILED_PRECONDITION" where !needsTotp: needsTotp = true
            case "UNAUTHENTICATED": self.error = needsTotp ? "That code was not accepted." : "Wrong handle or password."
            case "ALREADY_EXISTS": self.error = "That handle is taken."
            default: self.error = error.lumiaMessage
            }
        }
    }
}
