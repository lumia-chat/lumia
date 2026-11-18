// What is typed into a conversation's composer and not sent (issue #23,
// architecture 13, "Local-first"). The core keeps it, by conversation, in
// the instance's local store: the words and the reply they answer
// (`draft`, `setDraft`). It is this phone's alone and never sent to the
// instance, and the files waiting in the composer's strip are not kept
// with it. What is the phone's is when it is read and kept, which is
// `DraftKeeper`'s: read as the conversation comes on screen, kept once
// what is typed has rested for `DraftKeeper.rest`, at once when the
// conversation or the app leaves the screen, and let go as a message is
// sent. A read or a write that fails says nothing: the composer works as
// it did before there were drafts.
import Foundation
import LumiaFFI

/// The store's two calls as the keeper makes them: the draft kept for a
/// conversation, nil for none, and what is kept in its place, where words
/// that are empty or all blank keep nothing, the reply neither.
public struct DraftStore {
    public let read: @MainActor (_ channelID: String) throws -> DraftSummary?
    public let write: @MainActor (_ channelID: String, _ body: String, _ reply: QuoteSummary?) throws -> Void

    public init(read: @escaping @MainActor (String) throws -> DraftSummary?, write: @escaping @MainActor (String, String, QuoteSummary?) throws -> Void) {
        self.read = read; self.write = write
    }
}

public extension LumiaSession {
    /// This instance's drafts, in the core's store. Nothing is written once
    /// the account has left the phone: the core forgot every draft as it
    /// signed out, and a conversation screen going away after that must not
    /// put one back for whoever signs in next.
    var drafts: DraftStore {
        let client = client
        return DraftStore(read: { try client.draft(channelID: $0) }, write: { [weak self] channelID, body, reply in
            guard self?.signedIn == true else { return }
            try client.setDraft(channelID: channelID, body: body, reply: reply)
        })
    }
}

/// Keeps one conversation screen's draft. The screen says when its
/// conversation comes on screen (`open`, which answers the draft kept for
/// it), what its composer holds whenever that changes (`changed`), when
/// the conversation or the app leaves the screen (`leave`) and when a
/// message is sent (`sent`). What changed is written once it has rested
/// for `rest`, so a burst of keys is one write and a crash or a force-quit
/// loses at most that long; leaving writes at once, since iOS may end an
/// app in the background without a word; a send keeps nothing at once and
/// calls off the write that waits, so a late one cannot bring sent words
/// back. Nothing is written that the store already holds.
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

    private let wait: Wait
    private var store: DraftStore?
    /// The conversation on screen, nil before the first.
    public private(set) var channelID: String?
    /// What the composer holds.
    private var held = DraftSummary(body: "", reply: nil)
    /// What the store holds, as far as is known here.
    private var kept: DraftSummary?
    /// Calls off the write that waits for the words to rest.
    private var waiting: (@MainActor () -> Void)?

    /// `wait` is how a rested write is waited for: the tests' own, or a sleep.
    public init(wait: @escaping Wait = DraftKeeper.sleep) { self.wait = wait }

    /// The wait an app uses: a task that sleeps, called off by cancelling it.
    public nonisolated static let sleep: Wait = { pause, then in
        let task = Task { @MainActor in
            try? await Task.sleep(for: pause)
            if !Task.isCancelled { then() }
        }
        return { task.cancel() }
    }

    /// A conversation comes on screen: what the one before it held is kept
    /// at once, and the answer is what is kept for this one, for the
    /// screen to put in its field and reply bar; nil for nothing, and for a
    /// store that could not be read.
    public func open(_ channelID: String, in store: DraftStore) -> DraftSummary? {
        leave()
        self.channelID = channelID
        self.store = store
        kept = try? store.read(channelID)
        held = kept ?? DraftSummary(body: "", reply: nil)
        return kept
    }

    /// The composer's words or its reply changed: kept once they have
    /// rested, each change starting the wait again. What the store already
    /// holds waits for nothing, which is how the draft just put in the
    /// field, and the field a send emptied, write nothing.
    public func changed(body: String, reply: QuoteSummary?) {
        guard channelID != nil else { return }
        held = DraftSummary(body: body, reply: reply)
        waiting?()
        waiting = nil
        guard Self.worth(held) != kept else { return }
        waiting = wait(Self.rest) { [weak self] in self?.keep() }
    }

    /// The conversation leaves the screen, or the app the foreground: what
    /// waits is kept now.
    public func leave() {
        waiting?()
        keep()
    }

    /// A message was sent from the composer: nothing is kept for the
    /// conversation, and nothing waits to be. Words the screen puts back
    /// after a send that failed are a change like any other, kept again.
    public func sent() {
        waiting?()
        waiting = nil
        held = DraftSummary(body: "", reply: nil)
        guard let channelID, let store else { return }
        if (try? store.write(channelID, "", nil)) != nil { kept = nil }
    }

    /// Writes what the composer holds, unless the store holds it already. A
    /// write that fails leaves it to the next change or the next leaving.
    private func keep() {
        waiting = nil
        guard let channelID, let store, Self.worth(held) != kept else { return }
        if (try? store.write(channelID, held.body, held.reply)) != nil { kept = Self.worth(held) }
    }

    /// What the store keeps of what a composer holds: nothing for words
    /// that are empty or all blank, whatever they reply to (the core's rule).
    static func worth(_ draft: DraftSummary) -> DraftSummary? {
        draft.body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : draft
    }
}
