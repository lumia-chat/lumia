// Sign in on a phone: the instance (with its trust card, architecture
// 6.1), then a password with a second factor when the account asks for
// one, or a new account, or linking this phone from a device that holds
// the identity; the same flow as the web's and the desktop shell's.
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
    @State private var magicSent = false
    @State private var magicCode = ""
    @State private var busy = false

    var body: some View {
        ScrollView {
            VStack(spacing: LumiaTokens.space16) {
                Image("Logo").resizable().scaledToFit().frame(height: 96)
                    .padding(.top, LumiaTokens.space24)
                if let info {
                    accountCard(info)
                } else {
                    instanceCard
                }
                foot
                if let info, !info.privacyUrl.isEmpty || !info.termsUrl.isEmpty {
                    HStack(spacing: LumiaTokens.space4) {
                        if let url = URL(string: info.privacyUrl), !info.privacyUrl.isEmpty { Link("Privacy", destination: url) }
                        if let url = URL(string: info.termsUrl), !info.termsUrl.isEmpty { Link("Terms", destination: url) }
                    }
                    .buttonStyle(.lumiaLink)
                    .font(LumiaTokens.textMeta.font)
                }
            }
            .padding(LumiaTokens.space16)
        }
        .scrollDismissesKeyboard(.interactively)
        .task {
            if let saved = store.savedInstance { instance = saved }
            // Sent back here by the instance (a revoked session), the card opens saying so.
            if let notice = store.session?.takeEndedNotice() { error = notice }
            if store.session != nil { await reach() }
        }
    }

    private var instanceCard: some View {
        LumiaCard(title: "Welcome", subtitle: "Which instance do you want to sign in to?", width: nil) {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                LumiaField("Instance", text: $instance, placeholder: "https://chat.example.com")
                    .textInputAutocapitalization(.never).keyboardType(.URL).autocorrectionDisabled()
                    .onSubmit { Task { await connect() } }
                if !error.isEmpty { LumiaErrorText(error) }
                if let openError = store.openError, error.isEmpty { LumiaErrorText(openError) }
                if reaching {
                    Text("Reaching the instance…").font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                }
                Button { Task { await connect() } } label: { Text("Continue").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary)
                    .disabled(reaching)
            }
        }
    }

    private func accountCard(_ info: InstanceSummary) -> some View {
        LumiaCard(title: creating ? "Create an account" : "Welcome back", subtitle: creating ? "A new account on \(info.host)." : "Sign in to pick up your conversations.", width: nil) {
            VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                // The trust card as a row: the instance's tile, host, operator and version.
                HStack(spacing: LumiaTokens.space12) {
                    Text(initials(of: info.operatorName.isEmpty ? info.host : info.operatorName).prefix(1))
                        .font(LumiaTokens.titleMonogram.font).foregroundStyle(LumiaTokens.ink.color)
                        .frame(width: 34, height: 34)
                        .background(LumiaTokens.tile.color, in: Circle())
                    VStack(alignment: .leading, spacing: 0) {
                        Text(info.host).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkEmphasis.color).lineLimit(1)
                        Text("Run by \(info.operatorName.isEmpty ? "an unnamed operator" : info.operatorName)\(info.softwareVersion.isEmpty ? "" : " · Lumia \(info.softwareVersion)")")
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    Button("Change") { self.info = nil; error = "" }.buttonStyle(.lumiaLink)
                }
                .padding(.vertical, LumiaTokens.space6).padding(.horizontal, LumiaTokens.space10)
                .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))

                LumiaField(creating ? "Handle" : "Email or handle", text: $handle, placeholder: creating ? "yourname" : "you@example.com")
                    .textInputAutocapitalization(.never).autocorrectionDisabled().textContentType(.username)
                LumiaField("Password", text: $password, placeholder: creating ? "At least 12 characters" : "Your password", secure: true)
                    .textContentType(creating ? .newPassword : .password)
                    .onSubmit { Task { await submit() } }
                if needsTotp {
                    LumiaField("Verification code", text: $totp, placeholder: "6 digits from your authenticator")
                        .keyboardType(.numberPad).textContentType(.oneTimeCode)
                        .onSubmit { Task { await submit() } }
                }
                if !error.isEmpty { LumiaErrorText(error) }
                Button { Task { await submit() } } label: { Text(creating ? "Create account" : "Sign in").frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaPrimary)
                    .disabled(busy)
                if !creating, !info.oidcName.isEmpty {
                    Button { Task { await browserSignIn(oidc: true) } } label: { Label(info.oidcName, systemImage: "key").frame(maxWidth: .infinity) }
                        .buttonStyle(.lumiaSecondary).disabled(busy)
                }
                if !creating {
                    Button { Task { await browserSignIn(oidc: false) } } label: { Label("Sign in with a passkey", systemImage: "key").frame(maxWidth: .infinity) }
                        .buttonStyle(.lumiaSecondary).disabled(busy)
                }
                if !creating, info.magicLinkEnabled {
                    if magicSent {
                        LumiaNotice(icon: "envelope") { Text("A link is on its way to \(handle.trimmingCharacters(in: .whitespaces)) if it has an account here. Open it on this device, or paste its code below.") }
                        LumiaField("Code from the email", text: $magicCode, placeholder: "Paste the code")
                            .textInputAutocapitalization(.never).autocorrectionDisabled()
                        Button("Sign in with the code") { Task { await completeMagicLink() } }
                            .buttonStyle(.lumiaPrimary).disabled(busy || magicCode.trimmingCharacters(in: .whitespaces).isEmpty)
                    } else {
                        Button("Email me a sign-in link") { Task { await requestMagicLink() } }
                            .buttonStyle(.lumiaLink).disabled(busy || handle.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
                if !creating, info.identityProofSignInEnabled {
                    HStack(spacing: LumiaTokens.space12) {
                        Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
                        Text("or").font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        Rectangle().fill(LumiaTokens.line.color).frame(height: 1)
                    }
                    Button {
                        store.route = .link
                    } label: {
                        Label("Link this phone from another device", systemImage: "iphone").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.lumiaSecondary)
                }
            }
        }
    }

    @ViewBuilder
    private var foot: some View {
        if let info {
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
            .multilineTextAlignment(.center)
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

    private func requestMagicLink() async {
        guard let session = store.session else { return }
        error = ""
        busy = true
        defer { busy = false }
        do {
            try await session.requestMagicLink(email: handle)
            magicSent = true
        } catch {
            self.error = error.lumiaCode == "RATE_LIMITED" ? "Too many links in the last hour; try later." : error.lumiaMessage
        }
    }

    private func completeMagicLink() async {
        guard let session = store.session else { return }
        error = ""
        busy = true
        defer { busy = false }
        do {
            try await session.signInMagicLink(token: magicCode)
            store.route = .app
        } catch {
            self.error = error.lumiaCode == "UNAUTHENTICATED" ? "That code is unknown, used or expired. Ask for a new link." : error.lumiaMessage
        }
    }

    // Through the system browser (architecture 6.3): the provider, or the
    // instance's web sign-in page for a passkey; the sheet comes back with
    // a one-time token.
    private func browserSignIn(oidc: Bool) async {
        guard let session = store.session else { return }
        error = ""
        busy = true
        defer { busy = false }
        do {
            try await session.signInThroughBrowser(oidc: oidc)
            store.route = .app
        } catch BrowserSheetError.cancelled {
            // Dismissed: nothing to say.
        } catch {
            self.error = error.lumiaCode == "UNAUTHENTICATED" ? "That sign-in was not accepted." : error.lumiaMessage
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
            // The passkey factor needs the browser's ceremony (architecture 6.3): the web sign-in does it.
            case "FAILED_PRECONDITION" where error.lumiaDetails["mfa"] == "passkey": self.error = "This account asks for its passkey at sign-in; use \"Sign in through the browser\"."
            case "FAILED_PRECONDITION" where !needsTotp: needsTotp = true
            case "UNAUTHENTICATED": self.error = needsTotp ? "That code was not accepted." : "Wrong handle or password."
            case "ALREADY_EXISTS": self.error = "That handle is taken."
            default: self.error = error.lumiaMessage
            }
        }
    }
}
