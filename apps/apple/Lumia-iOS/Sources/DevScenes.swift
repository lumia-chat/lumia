// What a debug build draws with no instance, for looking at a screen in
// the simulator (`LUMIA_DEV_SCREEN=bodies`, `bodies-long` and
// `bodies-composer`): message bodies as the core
// reads them, every construct of the protocol's subset, a long one
// collapsed and one shown whole, and the composer near and over the limit.
// `reactions` is a few messages with reactions, a reply among them, drawn
// as the launch's preset says (`LUMIA_DEV_PRESET=minimal` hides them), and
// `reactions-preferences` the Preferences page where they are chosen.
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
#endif
