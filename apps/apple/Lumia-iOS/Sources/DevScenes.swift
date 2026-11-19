// What a debug build draws with no instance, for looking at a screen in
// the simulator (`LUMIA_DEV_SCREEN=bodies`, `bodies-long` and
// `bodies-composer`): message bodies as the core
// reads them, every construct of the protocol's subset, a long one
// collapsed and one shown whole, and the composer near and over the limit.
// `reactions` is a few messages with reactions, a reply among them, drawn
// as the launch's preset says (`LUMIA_DEV_PRESET=minimal` hides them), and
// `reactions-preferences` the Preferences page where they are chosen.
// `profile-updated` is a changed name drawn at once (issue #15).
// `tags` is a tag on every tone and at its longest (architecture 7.5).
// `linking-…` and `linkprompt-…` are linking by a prompt's stages.
// `restore-ask` is Restore's first question, `restore-keep` its devices to
// keep with none ticked and `restore-keep-1` with one; `removed` is the
// sign-in screen as a removed phone opens it. `viewer` and `viewer-small`
// are the viewer's picture, the largest a display picture is and one
// smaller than any phone, for looking at its zoom.
#if DEBUG
import LumiaFFI
import LumiaKit
import SwiftUI

struct BodiesScene: View {
    @Environment(Preferences.self) private var preferences
    @State private var expanded: Set<String> = ["whole"]
    @State private var near = String(repeating: "Lorem ipsum dolor sit amet. ", count: 530)
    @State private var over = String(repeating: "Lorem ipsum dolor sit amet. ", count: 586)
    @ScaledMetric(relativeTo: .body) private var systemText = LumiaTokens.textMessage.size

    /// A client on nothing, for the counter: the protocol's limit, which no welcome changes.
    private let client = try? LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: FileManager.default.temporaryDirectory.appendingPathComponent("lumia-scene"))

    private var measures: MessageMeasures { preferences.measures(system: systemText / LumiaTokens.textMessage.size) }

    private static let constructs = """
    Some *emphasis*, **strong**, ~~struck~~ and `inline code`, a [link](https://lumia.chat) and @ada.
    A line typed under it, and a word too long for any line: https://lumia.chat/a/very/long/path/that/does/not/stop/anywhere/soon

    > A quote, with **its own** styles.

    1. First
    2. Second
       - nested
    """

    private static let code = """
    Here is the fix:

    ```swift
    func room(_ body: String) -> MessageRoom { inner.messageRoom(body: body) } // a long line that scrolls sideways inside its block
    let x = 1
    ```
    """

    private static let long = (1...24).map { "Line \($0) of a message long enough to collapse." }.joined(separator: "\n")

    /// `bodies` the constructs and a code block, `bodies-long` a long
    /// message collapsed and one shown whole, `bodies-composer` the
    /// composer near the limit and over it.
    let part: String

    var body: some View {
        VStack(spacing: 0) {
            if part == "bodies-composer" {
                Spacer(minLength: 0)
                LumiaComposer(text: $near, placeholder: "Message #general", hint: nil, textScale: measures.text, room: { client?.messageRoom($0) ?? MessageRoom(shown: false, left: 0) }, send: {}) { EmptyView() }
                LumiaComposer(text: $over, placeholder: "Message #general", hint: nil, textScale: measures.text, room: { client?.messageRoom($0) ?? MessageRoom(shown: false, left: 0) }, send: {}) { EmptyView() }
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        if part == "bodies-long" {
                            row("long", "Ada Lovelace", Self.long)
                            row("whole", "Grace Hopper", Self.long)
                        } else {
                            row("constructs", "Ada Lovelace", Self.constructs, mentions: [MentionSummary(accountId: "ada", everyone: false, start: 92, end: 96)], edited: true)
                            row("code", "Grace Hopper", Self.code)
                        }
                    }
                    .padding(.vertical, LumiaTokens.space8)
                }
            }
        }
        .background(LumiaTokens.surface.color)
    }

    private func row(_ id: String, _ author: String, _ text: String, mentions: [MentionSummary] = [], edited: Bool = false) -> some View {
        var x = MessageExtras()
        x.mentions = mentions
        x.name = { $0 == "ada" ? "Ada Lovelace" : "" }
        x.edited = edited
        x.expanded = expanded.contains(id)
        x.expand = { expanded.formSymmetricDifference([id]) }
        return LumiaMessageRow(author: author, tone: .of(author), time: "12:04", text: text, measures: measures, extras: x)
    }
}

struct ReactionsScene: View {
    @Environment(Preferences.self) private var preferences
    @ScaledMetric(relativeTo: .body) private var systemText = LumiaTokens.textMessage.size

    private var measures: MessageMeasures { preferences.measures(system: systemText / LumiaTokens.textMessage.size) }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                LumiaDayDivider(label: "Today", measures: measures)
                row("Ada Lovelace", "12:01", "The new build is up. Grouped or one line, the chips sit under the words.", place: MessagePlace(closes: false), reactions: [("👍", 2, true, "You and Ada Lovelace reacted with 👍"), ("🎉", 1, false, "Ada Lovelace reacted with 🎉")])
                row("Ada Lovelace", "12:01", "A second message in the same block, with nothing on it.", place: MessagePlace(opens: false))
                row("Grace Hopper", "12:03", "Looks right here.", quote: "The new build is up.", reactions: [("🚀", 3, false, "Ada Lovelace, Grace Hopper and 1 other reacted with 🚀")])
                row("Grace Hopper", "12:04", "And the next message starts where the chips were when they are hidden.")
            }
            .padding(.vertical, LumiaTokens.space8)
        }
        .background(LumiaTokens.surface.color)
    }

    private func row(_ author: String, _ time: String, _ text: String, place: MessagePlace = MessagePlace(), quote: String? = nil, reactions: [(String, UInt32, Bool, String)] = []) -> some View {
        var x = MessageExtras()
        x.quote = quote.map { QuoteDisplay(authorID: "Ada Lovelace", name: "Ada Lovelace", text: $0, deleted: false, quotesMe: false, open: {}) }
        x.reactions = reactions.map { ReactionDisplay(emoji: $0.0, count: $0.1, me: $0.2, words: $0.3) }
        x.addReaction = {}
        x.actions = {}
        return LumiaMessageRow(author: author, tone: .of(author), time: time, text: text, place: place, measures: measures, extras: x)
    }
}

/// `tags`: the "Tags" board's figures with no instance (architecture 7.5):
/// a tag on each of the nine tones and on the plain chip, at a message's
/// size and at a member row's, a title's, and the accent's; then the
/// longest a tag can be, 32 characters, beside a name in a message and in a
/// member's row, where it gives way before the name does.
struct TagsScene: View {
    private let title = "Principal Reliability Engineer I"
    private let role = TagLook(label: "Community and Events Coordinator", tone: .green)

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                    ForEach([LumiaTag.Size.md, .sm], id: \.self) { size in
                        LumiaFlow(spacing: LumiaTokens.space6) {
                            ForEach(Roles.tones, id: \.self) { tone in LumiaTag(TagLook(label: tone.rawValue.capitalized, tone: tone), size: size) }
                            LumiaTag("No colour", size: size)
                            LumiaTag("Staff Engineer", size: size)
                            LumiaTag("You", tone: .accent, size: size)
                        }
                    }
                }
                .padding(.horizontal, LumiaTokens.space16)
                VStack(alignment: .leading, spacing: 0) {
                    LumiaMessageRow(author: "Devon Park", tone: .purple, time: "9:11 AM", text: "Looks great.", tag: TagLook(label: title, tone: nil))
                    LumiaMessageRow(author: "Devon Park", tone: .purple, time: "9:12 AM", text: "A role’s name can be as long.", tag: role)
                    LumiaMessageRow(author: "Maximiliana Featherstonehaugh", tone: .sky, time: "9:13 AM", text: "And a long name keeps its letters before the tag does.", tag: role)
                }
                VStack(alignment: .leading, spacing: 0) {
                    LumiaMemberRow(name: "Devon Park", tone: .purple, role: TagLook(label: title, tone: nil), status: "ONLINE")
                    LumiaMemberRow(name: "Priya Natarajan", tone: .amber, role: role, status: "ONLINE")
                    LumiaMemberRow(name: "Maximiliana Featherstonehaugh", tone: .sky, tag: "You", role: role, status: "ONLINE")
                    LumiaMemberRow(name: "Leo Martins", tone: .sky, role: TagLook(label: "Mobile Lead", tone: nil), status: "ONLINE")
                }
            }
            .padding(.vertical, LumiaTokens.space16)
        }
        .background(LumiaTokens.surface.color)
    }
}

/// `profile-updated`: a server's members and a few messages, and two
/// seconds in, Ada's `ProfileUpdate` (issue #15) written over the list as
/// the session writes it: her rows draw her new name and handle at once,
/// her server nickname stays, and Grace is as she was.
struct ProfileUpdatedScene: View {
    @State private var members = [
        MemberSummary(accountId: "ada", handle: "ada", displayName: "Ada", nickname: "Countess", pronouns: "", bio: "", avatarUrl: "", channelAddsByInvitation: false, serverDmsOff: false, roleIds: [], title: "", guest: false, owner: false, tag: nil, canManage: false),
        MemberSummary(accountId: "grace", handle: "grace", displayName: "Grace Hopper", nickname: "", pronouns: "", bio: "", avatarUrl: "", channelAddsByInvitation: false, serverDmsOff: false, roleIds: [], title: "", guest: false, owner: false, tag: nil, canManage: false),
    ]

    private func name(_ id: String) -> String {
        members.first { $0.accountId == id }.map { $0.displayName.isEmpty ? $0.handle : $0.displayName } ?? id
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(members, id: \.accountId) { m in
                    LumiaMemberRow(name: name(m.accountId), subtitle: "@\(m.handle) · nickname \(m.nickname.isEmpty ? "none" : m.nickname)", tone: .of(m.accountId))
                }
                LumiaDayDivider(label: "Today")
                LumiaMessageRow(author: name("ada"), tone: .of("ada"), time: "12:01", text: "Changing my name now.")
                LumiaMessageRow(author: name("grace"), tone: .of("grace"), time: "12:02", text: "Go on.")
            }
            .padding(.vertical, LumiaTokens.space8)
        }
        .background(LumiaTokens.surface.color)
        .task {
            try? await Task.sleep(for: .seconds(2))
            members = ProfileUpdate(accountID: "ada", handle: "countess", displayName: "Ada Lovelace", avatarURL: "", pronouns: "she/her", bio: "").applied(to: members)
        }
    }
}

/// Linking by a prompt (identity spec 4.2.1), each stage drawn from values:
/// `linking-waiting`, `-compare`, `-declined`, `-expired` and `-failed` are
/// the new device's link screen; `linkprompt-new`, `-web`, `-waiting`,
/// `-compare`, `-refused`, `-linked` and `-stopped` the prompt's sheet over
/// a phone that holds the identity.
struct RestoreStepsScene: View {
    let part: String
    @State private var kept: Set<String> = []

    private static func micros(_ month: Int, _ day: Int) -> Int64 {
        let date = Calendar.current.date(from: DateComponents(year: 2026, month: month, day: day, hour: 12))!
        return Int64(date.timeIntervalSince1970 * 1_000_000)
    }

    private static let devices = [
        DeviceSummary(deviceId: "mac", kind: "native", label: "MacBook Pro", webOrigin: "", addedAtMicros: micros(11, 12), thisDevice: false, revoked: false),
        DeviceSummary(deviceId: "phone", kind: "native", label: "iPhone", webOrigin: "", addedAtMicros: micros(11, 3), thisDevice: false, revoked: false),
        DeviceSummary(deviceId: "web", kind: "web", label: "Safari", webOrigin: "https://chat.acme.dev", addedAtMicros: micros(11, 1), thisDevice: false, revoked: false),
    ]

    var body: some View {
        if part == "restore-ask" {
            RestoreAskScreen(back: "Sign in", close: {}, link: {}, restore: {})
        } else {
            RestoreKeepScreen(devices: Self.devices, kept: $kept, busy: false, error: "", back: {}, restore: {})
                .onAppear { if part == "restore-keep-1" { kept = ["mac"] } }
        }
    }
}

/// The viewer's picture with no instance (specs/media.md 5): `viewer` is
/// 2,560 by 1,920 pixels, the largest a display picture is, in squares of
/// 320, and `viewer-small` 180 by 120, smaller than any phone, in squares
/// of 30. Every square is a grey of its own, so a screenshot says which
/// part of the picture is where and how large it is drawn, which is how
/// `scripts/smoke-viewer.sh` checks the zoom.
struct ViewerScene: View {
    private let picture: CGImage?
    private let pixels: CGSize

    init(part: String) {
        let (width, height, side) = part == "viewer-small" ? (180, 120, 30) : (2560, 1920, 320)
        picture = Self.squares(width: width, height: height, side: side)
        pixels = CGSize(width: width, height: height)
    }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if let picture { ViewerPicture(picture: picture, pixels: pixels, label: "Photo, squares") }
        }
    }

    /// The grey of the nth square, counted across and then down: 48 levels
    /// five apart from 20, in an order that puts no two alike side by side.
    static func grey(_ n: Int) -> Int { 20 + 5 * (n * 29 % 48) }

    private static func squares(width: Int, height: Int, side: Int) -> CGImage? {
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return nil }
        let across = width / side
        for row in 0..<height / side {
            for column in 0..<across {
                let grey = CGFloat(grey(row * across + column)) / 255
                context.setFillColor(red: grey, green: grey, blue: grey, alpha: 1)
                // A bitmap's rows count up from its bottom.
                context.fill(CGRect(x: column * side, y: height - (row + 1) * side, width: side, height: side))
            }
        }
        return context.makeImage()
    }
}

struct LinkStagesScene: View {
    let part: String

    private static func prompt(_ stage: String, kind: String = "native", label: String = "iPhone", origin: String = "") -> LinkPrompt {
        LinkPrompt(sessionId: "s", request: LinkRequestSummary(deviceId: "d", kind: kind, label: label, webOrigin: origin), stage: stage, digits: stage == "compare" ? "482 913" : "")
    }

    private var sheet: LinkByPrompt.Sheet? {
        switch part {
        case "linkprompt-new": .prompt(Self.prompt("new"))
        case "linkprompt-web": .prompt(Self.prompt("new", kind: "web", label: "Firefox", origin: "https://my.lumia.chat"))
        case "linkprompt-waiting": .prompt(Self.prompt("waiting"))
        case "linkprompt-compare": .prompt(Self.prompt("compare"))
        case "linkprompt-refused": .prompt(Self.prompt("refused"))
        case "linkprompt-linked": LinkByPrompt.sheet(prompts: [], ending: .linked(Self.prompt("new").request))
        case "linkprompt-stopped": .stopped
        default: nil
        }
    }

    private var stage: LinkByPrompt.Stage? {
        switch part {
        case "linking-waiting": .waiting
        case "linking-compare": .compare("482 913")
        case "linking-declined": .declined
        case "linking-expired": .expired
        default: nil
        }
    }

    var body: some View {
        if let sheet {
            LumiaEmptyState(icon: "lock", title: "Your conversations") { Text("What the phone was showing when the prompt came.") }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .sheet(isPresented: .constant(true)) { LinkPromptCard(sheet: sheet) { _, _ in } }
        } else {
            ScrollView {
                LumiaCard(title: "Link this phone", subtitle: "A device you already use approves this one.", width: nil) {
                    LinkByPromptSteps(stage: stage, error: part == "linking-failed" ? LinkByPrompt.startRefusal(LumiaError.Api(code: "RATE_LIMITED", detail: "", details: [:])) : nil, start: {}, cancel: {}, anotherWay: {})
                }
                .padding(LumiaTokens.space16)
                .padding(.top, LumiaTokens.space40)
            }
        }
    }
}
#endif
