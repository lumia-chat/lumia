// What is typed into a conversation's composer and not sent (issue #23,
// architecture 13, "Local-first"). The core keeps it, by conversation, in
// the instance's local store: the words and the reply they answer
// (`draft`, `setDraft`). It is this phone's alone and never sent to the
// instance, and the files waiting in the composer's strip are not kept
// with it. What is the phone's is when it is read and kept, which is
// `DraftKeeper`'s: read as the conversation comes on screen, kept once
// what is typed has rested for `DraftKeeper.rest`, at once when the
// conversation or the app leaves the screen, and let go once a message's
// send has returned without an error, not before. A read or a write that
// fails says nothing: the composer works as it did before there were drafts.
//
// Neither call is made on the main thread. Each takes the core's own lock
// and a write waits for the store's write lock, which the gateway holds
// while it applies a batch of what was missed and the store's other
// connections wait for up to ten seconds: on the main thread that is the
// keyboard and the list standing still for as long. `DraftStore.queued`
// runs them on a queue of their own, one at a time in the order asked, so
// a conversation's writes land in the order they were made and a read
// asked after a write sees it.
import Foundation
import LumiaFFI

/// The store's two calls as the keeper makes them, neither holding its
/// caller: the draft kept for a conversation, answered later with nil for
/// none and for a store that could not be read, and what is kept in its
/// place, where words that are empty or all blank keep nothing, the reply
/// neither, answered later with whether it was kept. Calls are carried out
/// in the order they are made, and each answers on the main actor.
public struct DraftStore {
    public typealias Read = @MainActor (_ channelID: String, _ answer: @escaping @MainActor (DraftSummary?) -> Void) -> Void
    public typealias Write = @MainActor (_ channelID: String, _ body: String, _ reply: QuoteSummary?, _ done: @escaping @MainActor (_ kept: Bool) -> Void) -> Void

    public let read: Read
    public let write: Write

    public init(read: @escaping Read, write: @escaping Write) {
        self.read = read; self.write = write
    }

    /// A store over two calls that block, the core's: each is run off the
    /// main thread on one queue, in the order asked. Nothing is written
    /// while `may` says no, which is asked as the write is.
    public static func queued(
        may: @escaping @MainActor () -> Bool = { true },
        read: @escaping @Sendable (_ channelID: String) throws -> DraftSummary?,
        write: @escaping @Sendable (_ channelID: String, _ body: String, _ reply: QuoteSummary?) throws -> Void
    ) -> DraftStore {
        let queue = DispatchQueue(label: "chat.lumia.drafts", qos: .userInitiated)
        return DraftStore(read: { channelID, answer in
            queue.async {
                let kept = try? read(channelID)
                DispatchQueue.main.async { MainActor.assumeIsolated { answer(kept) } }
            }
        }, write: { channelID, body, reply, done in
            guard may() else { return done(false) }
            queue.async {
                let kept = (try? write(channelID, body, reply)) != nil
                DispatchQueue.main.async { MainActor.assumeIsolated { done(kept) } }
            }
        })
    }
}

public extension LumiaSession {
    /// This instance's drafts, in the core's store, off the main thread.
    /// Nothing is written once the account has left the phone: the core
    /// forgot every draft as it signed out, and a conversation screen going
    /// away after that must not put one back for whoever signs in next.
    var drafts: DraftStore { draftKeeper.store }

    /// The store `draftKeeper` is made over, once for the session: its
    /// calls share one queue, which is what keeps them in order.
    internal func queuedDrafts() -> DraftStore {
        let client = client
        return .queued(
            may: { [weak self] in self?.signedIn == true },
            read: { try client.draft(channelID: $0) },
            write: { try client.setDraft(channelID: $0, body: $1, reply: $2) }
        )
    }
}

/// Keeps the drafts of the conversations on screen, one for a session. A
/// screen says when its conversation comes on screen (`open`, which puts
/// the draft kept for it in the screen's field), what its composer holds
/// whenever that changes (`changed`), when the app leaves the foreground
/// (`keepNow`), when the conversation leaves the screen (`leave`), and of
/// a message it sends, when it goes (`sending`) and what came of it
/// (`sent`, `unsent`). What changed is written once it has rested for
/// `rest`, so a burst of keys is one write and a crash or a force-quit
/// loses at most that long; leaving writes at once, since iOS may end an
/// app in the background without a word. Nothing is written that the store
/// already holds.
///
/// A message's words stay kept until its send has returned without an
/// error. The field empties as the send starts, and that is not written:
/// were the app killed before the core had the message, the words would be
/// in neither place. So `sending` keeps them, if a rested write had not
/// yet, `sent` lets them go, and `unsent` puts them back in the field when
/// nothing was typed there meanwhile, or leaves them kept for the
/// conversation's next opening when its screen has gone.
///
/// What is held of a conversation is held here and not by its screen, so
/// that a screen which goes while a send is on its way, and the one that
/// comes in its place, are told the same thing: the field is empty, and
/// the words are kept until the send returns.
///
/// An edit of a message is not a draft, and here needs no rule: a message
/// is edited in a field of its own where its words were, which the screen
/// never tells the keeper of.
@MainActor
public final class DraftKeeper {
    /// How long what is typed rests before it is kept.
    public static let rest: Duration = .milliseconds(400)

    /// Runs `then` once `pause` has passed, unless what it answers is called first.
    public typealias Wait = @MainActor (_ pause: Duration, _ then: @escaping @MainActor () -> Void) -> @MainActor () -> Void

    /// Puts a draft in a screen's field and reply bar, or empties them for nil.
    public typealias Place = @MainActor (DraftSummary?) -> Void

    /// What is held of one conversation while it is on screen or a send from it has not returned.
    private final class Conversation {
        /// What the composer holds.
        var held = DraftKeeper.nothing
        /// What the store holds, or was last asked to; nothing to go by until `known`.
        var kept: DraftSummary?
        /// The store has answered what it kept, or has been written to since.
        var known = false
        /// The last write did not go: the next change or leaving writes again.
        var failed = false
        /// How many writes were asked, to tell the last one's answer.
        var asked = 0
        /// Calls off the write that waits for the words to rest.
        var waiting: (@MainActor () -> Void)?
        /// The sends from here that have not returned.
        var flying = 0
        /// The screens showing it.
        var screens = 0
        /// The screen's field, while one shows it.
        var place: Place?
    }

    public let store: DraftStore
    private let wait: Wait
    private var conversations: [String: Conversation] = [:]
    /// The writes asked of the store that it has not answered.
    private var writes = 0
    private var settling: [CheckedContinuation<Void, Never>] = []

    /// `wait` is how a rested write is waited for: the tests' own, or a sleep.
    public init(store: DraftStore, wait: @escaping Wait = DraftKeeper.sleep) {
        self.store = store; self.wait = wait
    }

    /// The wait an app uses: a task that sleeps, called off by cancelling it.
    public nonisolated static let sleep: Wait = { pause, then in
        let task = Task { @MainActor in
            try? await Task.sleep(for: pause)
            if !Task.isCancelled { then() }
        }
        return { task.cancel() }
    }

    /// A conversation comes on screen. Its field is emptied at once, and
    /// the draft kept for it is put there when the store answers, unless
    /// something was typed first, which is newer. A conversation still held
    /// here (a send from it has not returned) is given what its composer
    /// held. With `holding`, the screen is back from under another with
    /// its field as it was left: nothing is put in it but a draft kept
    /// where it holds nothing.
    public func open(_ channelID: String, holding: DraftSummary? = nil, place: @escaping Place) {
        if let c = conversations[channelID] {
            c.screens += 1
            c.place = place
            if let holding {
                changed(channelID, body: holding.body, reply: holding.reply)
            } else {
                place(c.held == Self.nothing ? nil : c.held)
            }
            return
        }
        let c = Conversation()
        c.screens = 1
        c.place = place
        if let holding { c.held = holding } else { place(nil) }
        conversations[channelID] = c
        store.read(channelID) { [weak self, weak c] kept in
            guard let self, let c, self.conversations[channelID] === c, !c.known else { return }
            c.known = true
            c.kept = kept
            guard let kept, Self.worth(c.held) == nil else { return }
            c.held = kept
            c.place?(kept)
        }
    }

    /// The composer's words or its reply changed: kept once they have
    /// rested, each change starting the wait again. What the store already
    /// holds waits for nothing, which is how the draft just put in the
    /// field writes nothing, and neither does the field a send emptied
    /// while that send has not returned.
    public func changed(_ channelID: String, body: String, reply: QuoteSummary?) {
        guard let c = conversations[channelID] else { return }
        let now = DraftSummary(body: body, reply: reply)
        guard now != c.held else { return }
        c.held = now
        c.waiting?()
        c.waiting = nil
        guard needsWrite(c) else { return }
        c.waiting = wait(Self.rest) { [weak self, weak c] in
            guard let self, let c else { return }
            c.waiting = nil
            self.keep(channelID, c)
        }
    }

    /// The app leaves the foreground: what waits is kept now.
    public func keepNow(_ channelID: String) {
        guard let c = conversations[channelID] else { return }
        c.waiting?()
        c.waiting = nil
        keep(channelID, c)
    }

    /// The conversation leaves the screen: what waits is kept now, and
    /// nothing more is held of it here unless a send from it has not returned.
    public func leave(_ channelID: String) {
        guard let c = conversations[channelID] else { return }
        keepNow(channelID)
        c.screens = max(0, c.screens - 1)
        if c.screens == 0 { c.place = nil }
        drop(channelID, c)
    }

    /// A message goes from the composer, which held `body` and `reply` and
    /// is emptied for it. They stay kept until the send returns, and are
    /// kept now if they had not rested yet.
    public func sending(_ channelID: String, body: String, reply: QuoteSummary?) {
        guard let c = conversations[channelID] else { return }
        c.waiting?()
        c.waiting = nil
        c.held = DraftSummary(body: body, reply: reply)
        keep(channelID, c)
        c.flying += 1
    }

    /// A send returned without an error: nothing is kept for the
    /// conversation, unless something was typed there meanwhile, which is
    /// its draft now, or another send from it has not returned.
    public func sent(_ channelID: String) {
        guard let c = conversations[channelID] else { return }
        c.flying = max(0, c.flying - 1)
        if c.flying == 0, Self.worth(c.held) == nil {
            c.waiting?()
            c.waiting = nil
            keep(channelID, c)
        }
        drop(channelID, c)
    }

    /// A send failed: its words are the composer's again where nothing was
    /// typed meanwhile, put back in the field of the screen that shows the
    /// conversation, and kept for its next opening when none does.
    public func unsent(_ channelID: String, body: String, reply: QuoteSummary?) {
        guard let c = conversations[channelID] else { return }
        c.flying = max(0, c.flying - 1)
        let words = DraftSummary(body: body, reply: reply)
        if Self.worth(c.held) == nil, Self.worth(words) != nil {
            c.waiting?()
            c.waiting = nil
            c.held = words
            c.place?(words)
            keep(channelID, c)
        }
        drop(channelID, c)
    }

    /// The account has left the phone: nothing is held of its
    /// conversations, and nothing waits to be written.
    public func forget() {
        for c in conversations.values { c.waiting?() }
        conversations = [:]
    }

    /// Whether the store has yet to answer a write.
    public var writing: Bool { writes > 0 }

    /// Returns once the store has answered every write asked of it, which
    /// an app going to the background waits for before it is suspended.
    public func settled() async {
        guard writes > 0 else { return }
        await withCheckedContinuation { settling.append($0) }
    }

    /// Whether what the composer holds is to be written.
    private func needsWrite(_ c: Conversation) -> Bool {
        let worth = Self.worth(c.held)
        // Emptied for a send that has not returned: its words stay kept.
        if worth == nil, c.flying > 0 { return false }
        // The store has not answered yet: only something typed is newer than whatever it holds.
        guard c.known else { return worth != nil }
        return c.failed || worth != c.kept
    }

    /// Writes what the composer holds, unless the store holds it already. A
    /// write that fails leaves it to the next change or the next leaving.
    private func keep(_ channelID: String, _ c: Conversation) {
        guard needsWrite(c) else { return }
        let draft = Self.worth(c.held)
        c.kept = draft
        c.known = true
        c.failed = false
        c.asked += 1
        let asked = c.asked
        writes += 1
        store.write(channelID, draft?.body ?? "", draft?.reply) { [weak self, weak c] kept in
            if !kept, let c, c.asked == asked { c.failed = true }
            guard let self else { return }
            self.writes -= 1
            guard self.writes == 0 else { return }
            let waiting = self.settling
            self.settling = []
            for continuation in waiting { continuation.resume() }
        }
    }

    /// Lets go of a conversation no screen shows and no send is out from.
    private func drop(_ channelID: String, _ c: Conversation) {
        guard c.screens == 0, c.flying == 0 else { return }
        c.waiting?()
        conversations[channelID] = nil
    }

    /// An empty composer.
    nonisolated static let nothing = DraftSummary(body: "", reply: nil)

    /// What the store keeps of what a composer holds: nothing for words
    /// that are empty or all blank, whatever they reply to (the core's rule).
    static func worth(_ draft: DraftSummary) -> DraftSummary? {
        draft.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : draft
    }
}
