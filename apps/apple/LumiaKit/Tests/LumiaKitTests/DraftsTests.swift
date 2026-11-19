// What is typed into a composer and not sent (issue #23): a kept draft put
// in the field as its conversation comes on screen, a burst of changes
// written once when it rests, leaving written at once, a message's words
// kept until its send has returned and let go only then, a send that
// fails, a screen that goes while a send is on its way, an emptied field,
// the reply kept with the words, a store that fails, a store that is slow
// to answer, an account that has left, and the core's store itself: read
// by a second client on the same files, and written to off the main
// thread while another writer holds its lock.
@testable import LumiaKit
import LumiaFFI
import XCTest

final class DraftsTests: XCTestCase {
    /// A store in memory that keeps as the core's does (blank words keep
    /// nothing), lists its writes and fails when told to. While `slow`, it
    /// answers nothing until `answer()`, and then every call in the order
    /// it was made, as the queue the core's calls run on does.
    @MainActor
    private final class Store {
        var kept: [String: DraftSummary] = [:]
        var writes: [String] = []
        var failing = false
        var slow = false
        private var asked: [@MainActor () -> Void] = []

        var calls: DraftStore {
            DraftStore(read: { [self] id, answer in
                run { [self] in answer(failing ? nil : kept[id]) }
            }, write: { [self] id, body, reply, done in
                run { [self] in
                    if failing { return done(false) }
                    writes.append("\(id): \(body)")
                    kept[id] = body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : DraftSummary(body: body, reply: reply)
                    done(true)
                }
            })
        }

        private func run(_ call: @escaping @MainActor () -> Void) {
            if slow { asked.append(call) } else { call() }
        }

        func answer() {
            let due = asked
            asked = []
            for call in due { call() }
        }
    }

    /// The wait for the words to rest, passed by hand: `rest()` is the
    /// pause running out for every wait not called off.
    @MainActor
    private final class Pauses {
        private var waits: [Int: @MainActor () -> Void] = [:]
        private var next = 0
        var asked: [Duration] = []
        var waiting: Int { waits.count }

        var wait: DraftKeeper.Wait {
            { [self] pause, then in
                next += 1
                let n = next
                asked.append(pause)
                waits[n] = then
                return { [self] in waits[n] = nil }
            }
        }

        func rest() {
            let due = waits.sorted { $0.key < $1.key }.map(\.value)
            waits = [:]
            for then in due { then() }
        }
    }

    /// A screen's field and reply bar: what was put in them, in order, nil for emptied.
    @MainActor
    private final class Field {
        var placed: [DraftSummary?] = []
        var place: DraftKeeper.Place { { [self] in placed.append($0) } }
    }

    private func quote(_ id: String = "m1", silent: Bool = false) -> QuoteSummary {
        Replies.quote(messageID: id, authorID: "ada", body: "Shall we ship on Friday?", silent: silent)
    }

    @MainActor
    func testAKeptDraftIsPutInTheFieldAsItsConversationComesOnScreen() {
        let (store, pauses, field) = (Store(), Pauses(), Field())
        let kept = DraftSummary(body: "Half a thought", reply: quote())
        store.kept["design"] = kept
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        // The field is emptied for the conversation, then given what was kept for it.
        keeper.open("design", place: field.place)
        XCTAssertEqual(field.placed, [nil, kept])
        // The screen puts it in its field and reply bar, which is a change
        // of what the composer holds and nothing to write.
        keeper.changed("design", body: kept.body, reply: kept.reply)
        XCTAssertEqual(pauses.waiting, 0)
        keeper.leave("design")
        XCTAssertEqual(store.writes, [])
        // Another conversation has its own, or none: the first one's words do not follow there.
        let other = Field()
        keeper.open("random", place: other.place)
        XCTAssertEqual(other.placed, [nil])
        XCTAssertEqual(store.writes, [])
        XCTAssertEqual(store.kept["design"], kept)
    }

    @MainActor
    func testABurstOfChangesIsOneWriteOnceItRests() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        keeper.open("design", place: Field().place)
        for words in ["H", "Ha", "Half", "Half a thought"] { keeper.changed("design", body: words, reply: nil) }
        // Each change calls off the wait before it: one is left, and nothing is written yet.
        XCTAssertEqual(pauses.waiting, 1)
        XCTAssertEqual(pauses.asked, Array(repeating: .milliseconds(400), count: 4))
        XCTAssertEqual(store.writes, [])
        pauses.rest()
        XCTAssertEqual(store.writes, ["design: Half a thought"])
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "Half a thought", reply: nil))
        // Rested and kept: nothing more to write, now or as the app leaves the foreground.
        pauses.rest()
        keeper.keepNow("design")
        XCTAssertEqual(store.writes.count, 1)
        // A key after that is the next burst.
        keeper.changed("design", body: "Half a thought.", reply: nil)
        pauses.rest()
        XCTAssertEqual(store.writes, ["design: Half a thought", "design: Half a thought."])
    }

    @MainActor
    func testLeavingKeepsAtOnce() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        keeper.open("design", place: Field().place)
        keeper.changed("design", body: "Not rested yet", reply: nil)
        // The app leaves the foreground: written now, with nothing left waiting.
        keeper.keepNow("design")
        XCTAssertEqual(store.writes, ["design: Not rested yet"])
        XCTAssertEqual(pauses.waiting, 0)
        pauses.rest()
        keeper.keepNow("design")
        XCTAssertEqual(store.writes.count, 1)

        // Another conversation in the same screen: the first one's words are
        // kept under its own name before the next is read.
        keeper.changed("design", body: "Not rested yet, still", reply: nil)
        keeper.leave("design")
        let random = Field()
        keeper.open("random", place: random.place)
        XCTAssertEqual(random.placed, [nil])
        XCTAssertEqual(store.writes, ["design: Not rested yet", "design: Not rested yet, still"])
        XCTAssertEqual(pauses.waiting, 0)
        keeper.changed("random", body: "Elsewhere", reply: nil)
        keeper.leave("random")
        XCTAssertEqual(store.kept["random"]?.body, "Elsewhere")
        XCTAssertEqual(store.kept["design"]?.body, "Not rested yet, still")
        let design = Field()
        keeper.open("design", place: design.place)
        XCTAssertEqual(design.placed.last??.body, "Not rested yet, still")
        // Left, nothing more is told of it: a change that comes late is not written.
        keeper.leave("design")
        keeper.changed("design", body: "After it went", reply: nil)
        XCTAssertEqual(pauses.waiting, 0)
    }

    // Back from a screen pushed over the conversation, its field is as it
    // was left: nothing is put in it, and what is typed next is kept.
    @MainActor
    func testAFieldBackFromUnderAnotherScreenIsLeftAsItWas() {
        let (store, pauses, field) = (Store(), Pauses(), Field())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        keeper.open("design", place: field.place)
        keeper.changed("design", body: "Half", reply: quote())
        keeper.leave("design")
        XCTAssertEqual(store.writes, ["design: Half"])
        keeper.open("design", holding: DraftSummary(body: "Half", reply: quote()), place: field.place)
        XCTAssertEqual(field.placed, [nil], "nothing was put in a field that was left as it was")
        keeper.changed("design", body: "Half", reply: quote())
        XCTAssertEqual(pauses.waiting, 0)
        keeper.changed("design", body: "Half a thought", reply: quote())
        pauses.rest()
        XCTAssertEqual(store.writes, ["design: Half", "design: Half a thought"])
        // A write that failed as it left: back, the field still holds what
        // the store does not, which is not written over and is kept at the
        // next leaving.
        store.failing = true
        keeper.changed("design", body: "Half a thought, more", reply: quote())
        keeper.leave("design")
        store.failing = false
        keeper.open("design", holding: DraftSummary(body: "Half a thought, more", reply: quote()), place: field.place)
        XCTAssertEqual(field.placed, [nil], "the store's older words were put over the field's")
        keeper.leave("design")
        XCTAssertEqual(store.kept["design"]?.body, "Half a thought, more")
        // Left empty and back empty, with a draft kept meanwhile (a send
        // that failed while the screen was under another): it is the field's.
        keeper.open("design", holding: DraftSummary(body: "", reply: nil), place: field.place)
        XCTAssertEqual(field.placed.last??.body, "Half a thought, more")
    }

    // The words of a message stay kept until its send has returned without
    // an error: emptied for the send, the field is not what is kept, so an
    // app ended while the send is on its way still has them.
    @MainActor
    func testASendsWordsAreKeptUntilItHasReturned() {
        let (store, pauses, field) = (Store(), Pauses(), Field())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        keeper.open("design", place: field.place)
        keeper.changed("design", body: "Shipping on Friday", reply: quote())
        pauses.rest()
        XCTAssertNotNil(store.kept["design"])
        // More typed, then sent before it rested: what goes is kept now.
        keeper.changed("design", body: "Shipping on Friday.", reply: quote())
        XCTAssertEqual(pauses.waiting, 1)
        keeper.sending("design", body: "Shipping on Friday.", reply: quote())
        XCTAssertEqual(pauses.waiting, 0, "the write that waited was not called off")
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "Shipping on Friday.", reply: quote()))
        // The field the send emptied is nothing to write, rested, in the
        // background or gone from the screen: the app may be ended here.
        keeper.changed("design", body: "", reply: nil)
        XCTAssertEqual(pauses.waiting, 0)
        pauses.rest()
        keeper.keepNow("design")
        XCTAssertEqual(store.kept["design"]?.body, "Shipping on Friday.", "the draft was let go before the send returned")
        // Returned without an error: let go, at once.
        keeper.sent("design")
        XCTAssertNil(store.kept["design"])
        XCTAssertEqual(store.writes, ["design: Shipping on Friday", "design: Shipping on Friday.", "design: "])
        pauses.rest()
        keeper.leave("design")
        XCTAssertNil(store.kept["design"])
        XCTAssertEqual(store.writes.count, 3)

        // Sent within the pause of the first key: kept for as long as the send takes, then not.
        let random = Field()
        keeper.open("random", place: random.place)
        keeper.changed("random", body: "ok", reply: nil)
        keeper.sending("random", body: "ok", reply: nil)
        XCTAssertEqual(store.kept["random"]?.body, "ok")
        keeper.changed("random", body: "", reply: nil)
        keeper.sent("random")
        pauses.rest()
        XCTAssertNil(store.kept["random"])
        XCTAssertEqual(random.placed, [nil], "a send that went put something in the field")
    }

    // A send that fails: the words and the reply are still kept, and are
    // the field's again, unless something was typed there meanwhile.
    @MainActor
    func testASendThatFailsLeavesItsWordsInTheFieldAndTheDraft() {
        let (store, pauses, field) = (Store(), Pauses(), Field())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        let words = DraftSummary(body: "Shipping on Friday.", reply: quote(silent: true))
        keeper.open("design", place: field.place)
        keeper.changed("design", body: words.body, reply: words.reply)
        keeper.sending("design", body: words.body, reply: words.reply)
        keeper.changed("design", body: "", reply: nil)
        keeper.unsent("design", body: words.body, reply: words.reply)
        XCTAssertEqual(field.placed, [nil, words], "the words did not come back to the field")
        XCTAssertEqual(store.kept["design"], words)
        XCTAssertEqual(store.writes, ["design: Shipping on Friday."], "kept once, as it went")
        // In the field again, they are what the composer holds: nothing to write, and a key after is.
        keeper.changed("design", body: words.body, reply: words.reply)
        XCTAssertEqual(pauses.waiting, 0)
        keeper.changed("design", body: "Shipping on Friday, I think.", reply: words.reply)
        pauses.rest()
        XCTAssertEqual(store.kept["design"]?.body, "Shipping on Friday, I think.")

        // Something new typed while it was on its way: that is the draft, and the field is left alone.
        keeper.sending("design", body: "Shipping on Friday, I think.", reply: words.reply)
        keeper.changed("design", body: "", reply: nil)
        keeper.changed("design", body: "And another thing", reply: nil)
        pauses.rest()
        keeper.unsent("design", body: "Shipping on Friday, I think.", reply: words.reply)
        XCTAssertEqual(field.placed.count, 2)
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "And another thing", reply: nil))
        // And when such a send goes through, what was typed meanwhile is not let go with it.
        keeper.sending("design", body: "And another thing", reply: nil)
        keeper.changed("design", body: "", reply: nil)
        keeper.changed("design", body: "Next", reply: nil)
        keeper.sent("design")
        pauses.rest()
        XCTAssertEqual(store.kept["design"]?.body, "Next")
    }

    // The screen goes while a send is on its way, and comes back, or
    // another comes in its place: its field is empty, not holding words
    // already sent, and they are let go, or kept, as the send comes out.
    @MainActor
    func testAScreenThatGoesWhileASendIsOnItsWay() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        let first = Field()
        keeper.open("design", place: first.place)
        keeper.changed("design", body: "On my way", reply: nil)
        keeper.sending("design", body: "On my way", reply: nil)
        keeper.changed("design", body: "", reply: nil)
        keeper.leave("design")
        XCTAssertEqual(store.kept["design"]?.body, "On my way", "leaving let the words go before the send returned")
        // Another screen of it: empty, though the store still holds the words.
        let second = Field()
        keeper.open("design", place: second.place)
        XCTAssertEqual(second.placed, [nil])
        keeper.sent("design")
        XCTAssertNil(store.kept["design"])
        XCTAssertEqual(second.placed, [nil])
        XCTAssertEqual(first.placed, [nil])

        // It fails with a screen of the conversation there: the words are that screen's.
        keeper.changed("design", body: "Again", reply: nil)
        keeper.sending("design", body: "Again", reply: nil)
        keeper.changed("design", body: "", reply: nil)
        keeper.leave("design")
        let third = Field()
        keeper.open("design", place: third.place)
        keeper.unsent("design", body: "Again", reply: nil)
        XCTAssertEqual(third.placed, [nil, DraftSummary(body: "Again", reply: nil)])
        XCTAssertEqual(second.placed, [nil], "a screen that had gone was given the words")

        // It fails with no screen of it: kept, and there when it is next opened.
        keeper.sending("design", body: "Again", reply: nil)
        keeper.changed("design", body: "", reply: nil)
        keeper.leave("design")
        keeper.unsent("design", body: "Again", reply: nil)
        XCTAssertEqual(store.kept["design"]?.body, "Again")
        let fourth = Field()
        keeper.open("design", place: fourth.place)
        XCTAssertEqual(fourth.placed, [nil, DraftSummary(body: "Again", reply: nil)])
        // And it goes through with no screen of it: let go.
        keeper.sending("design", body: "Again", reply: nil)
        keeper.changed("design", body: "", reply: nil)
        keeper.leave("design")
        keeper.sent("design")
        XCTAssertNil(store.kept["design"])
        XCTAssertEqual(pauses.waiting, 0)
    }

    @MainActor
    func testAnEmptiedFieldKeepsNothing() {
        let (store, pauses) = (Store(), Pauses())
        store.kept["design"] = DraftSummary(body: "Half a thought", reply: quote())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        keeper.open("design", place: Field().place)
        // All blank is empty, and its reply is not kept without words.
        keeper.changed("design", body: " \n", reply: quote())
        pauses.rest()
        XCTAssertNil(store.kept["design"])
        XCTAssertEqual(store.writes.count, 1)
        // A reply chosen before a word is typed is nothing to keep yet.
        keeper.changed("design", body: "", reply: quote("m2"))
        XCTAssertEqual(pauses.waiting, 0)
        keeper.leave("design")
        XCTAssertEqual(store.writes.count, 1)
    }

    @MainActor
    func testTheReplyIsKeptWithTheWords() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        keeper.open("design", place: Field().place)
        keeper.changed("design", body: "On it", reply: nil)
        pauses.rest()
        // The reply alone is a change: begun, its "@ On" turned off, cancelled.
        keeper.changed("design", body: "On it", reply: quote())
        pauses.rest()
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "On it", reply: quote()))
        keeper.changed("design", body: "On it", reply: quote(silent: true))
        pauses.rest()
        XCTAssertEqual(store.kept["design"]?.reply?.silent, true)
        keeper.changed("design", body: "On it", reply: nil)
        keeper.leave("design")
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "On it", reply: nil))
        XCTAssertEqual(store.writes.count, 4)
    }

    @MainActor
    func testAStoreThatFailsIsSilentAndAskedAgain() {
        let (store, pauses, field) = (Store(), Pauses(), Field())
        store.kept["design"] = DraftSummary(body: "Kept before", reply: nil)
        store.failing = true
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        // Not read: the field opens empty, as it did before there were drafts.
        keeper.open("design", place: field.place)
        XCTAssertEqual(field.placed, [nil])
        keeper.changed("design", body: "Typed anyway", reply: nil)
        pauses.rest()
        keeper.sending("design", body: "Typed anyway", reply: nil)
        keeper.changed("design", body: "", reply: nil)
        keeper.sent("design")
        keeper.changed("design", body: "And again", reply: nil)
        pauses.rest()
        XCTAssertEqual(store.writes, [])
        // What could not be written is written when the store next takes it.
        store.failing = false
        keeper.keepNow("design")
        XCTAssertEqual(store.kept["design"]?.body, "And again")
        // A send's letting go that failed is made again too.
        keeper.sending("design", body: "And again", reply: nil)
        keeper.changed("design", body: "", reply: nil)
        store.failing = true
        keeper.sent("design")
        XCTAssertEqual(store.kept["design"]?.body, "And again")
        store.failing = false
        keeper.leave("design")
        XCTAssertNil(store.kept["design"])
    }

    @MainActor
    func testNothingIsToldBeforeAConversationIsOnScreen() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        keeper.changed("design", body: "Nowhere", reply: nil)
        keeper.sending("design", body: "Nowhere", reply: nil)
        keeper.sent("design")
        keeper.unsent("design", body: "Nowhere", reply: nil)
        keeper.keepNow("design")
        keeper.leave("design")
        XCTAssertEqual(pauses.waiting, 0)
        XCTAssertEqual(store.writes, [])
    }

    // The store answers later, as the core's does from its own queue:
    // nothing waits for it, what was typed before it answered is newer
    // than what it kept, and its writes land in the order they were made.
    @MainActor
    func testAStoreSlowToAnswerHoldsNothingUp() async {
        let (store, pauses, field) = (Store(), Pauses(), Field())
        store.kept["design"] = DraftSummary(body: "Kept before", reply: quote())
        store.slow = true
        let keeper = DraftKeeper(store: store.calls, wait: pauses.wait)
        // Not answered yet: the field is empty, and leaving it so writes nothing over what is kept.
        keeper.open("design", place: field.place)
        XCTAssertEqual(field.placed, [nil])
        keeper.keepNow("design")
        keeper.leave("design")
        store.answer()
        XCTAssertEqual(store.writes, [])
        XCTAssertEqual(field.placed, [nil], "a screen that had gone was given a draft")
        // Answered with nothing typed: put in the field.
        keeper.open("design", place: field.place)
        store.answer()
        XCTAssertEqual(field.placed.last, DraftSummary(body: "Kept before", reply: quote()))
        keeper.changed("design", body: "Kept before", reply: quote())
        keeper.leave("design")

        // Typed before it answered: the field is not written over, and what was typed is kept.
        let typed = Field()
        keeper.open("design", place: typed.place)
        keeper.changed("design", body: "Typed first", reply: nil)
        store.answer()
        XCTAssertEqual(typed.placed, [nil], "what the store kept was put over what was typed")
        pauses.rest()
        XCTAssertTrue(keeper.writing)
        store.answer()
        XCTAssertFalse(keeper.writing)
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "Typed first", reply: nil))

        // Written in the order made, and `settled` returns once every one is answered.
        keeper.changed("design", body: "Typed first, then more", reply: nil)
        keeper.keepNow("design")
        keeper.changed("design", body: "Typed first, then more, and more", reply: nil)
        keeper.keepNow("design")
        XCTAssertEqual(store.writes, ["design: Typed first"])
        var settled = false
        let waiting = Task { @MainActor in
            await keeper.settled()
            settled = true
        }
        await Task.yield()
        XCTAssertFalse(settled, "settled before the store had answered")
        store.answer()
        await waiting.value
        XCTAssertTrue(settled)
        XCTAssertEqual(store.writes, ["design: Typed first", "design: Typed first, then more", "design: Typed first, then more, and more"])
        await keeper.settled()

        // Left and opened again before the first read answered: that answer
        // is of a store since written to, and is not what goes in the field.
        store.kept["design"] = nil
        keeper.leave("design")
        store.answer()
        let again = Field()
        keeper.open("design", place: again.place)
        keeper.changed("design", body: "x", reply: nil)
        keeper.leave("design")
        keeper.open("design", place: again.place)
        store.answer()
        XCTAssertEqual(again.placed, [nil, nil, DraftSummary(body: "x", reply: nil)])
    }

    // The wait an app uses is a sleep: what is typed is in the store soon
    // after it rests, and one called off writes nothing.
    @MainActor
    func testTheRestedWriteIsASleep() async throws {
        let store = Store()
        let keeper = DraftKeeper(store: store.calls)
        keeper.open("design", place: Field().place)
        keeper.changed("design", body: "Half", reply: nil)
        keeper.changed("design", body: "Half a thought", reply: nil)
        XCTAssertEqual(store.writes, [])
        for _ in 0..<100 where store.writes.isEmpty { try await Task.sleep(for: .milliseconds(50)) }
        XCTAssertEqual(store.writes, ["design: Half a thought"])
        keeper.changed("design", body: "Half a thought, sent", reply: nil)
        keeper.sending("design", body: "Half a thought, sent", reply: nil)
        keeper.changed("design", body: "", reply: nil)
        keeper.sent("design")
        try await Task.sleep(for: DraftKeeper.rest * 2)
        XCTAssertEqual(store.writes, ["design: Half a thought", "design: Half a thought, sent", "design: "])
    }

    // The core's own store, with no instance: what one client keeps is
    // there for another opened on the same files, which is what a launch
    // after a quit or a crash is, by conversation, with its reply, until it
    // is emptied.
    func testADraftIsThereForTheNextClientOnTheSameFiles() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let base = URL(string: "http://127.0.0.1:1")!
        let (here, there) = ("01890000-0000-7000-8000-000000000031", "01890000-0000-7000-8000-000000000032")
        let reply = Replies.quote(messageID: "01890000-0000-7000-8000-000000000041", authorID: "01890000-0000-7000-8000-000000000051", body: "Shall we?", silent: true)
        let first = try LumiaClient(baseURL: base, dataDirectory: dir)
        XCTAssertNil(try first.draft(channelID: here))
        try first.setDraft(channelID: here, body: "Half a thought\n", reply: reply)
        try first.setDraft(channelID: there, body: "Elsewhere", reply: nil)

        // The first is still open, as a process that was killed never closed its own.
        let second = try LumiaClient(baseURL: base, dataDirectory: dir)
        XCTAssertEqual(try second.draft(channelID: here), DraftSummary(body: "Half a thought\n", reply: reply))
        XCTAssertEqual(try second.draft(channelID: there), DraftSummary(body: "Elsewhere", reply: nil))
        try second.setDraft(channelID: here, body: " ", reply: reply)
        XCTAssertNil(try first.draft(channelID: here))
        XCTAssertEqual(try first.draft(channelID: there)?.body, "Elsewhere")
        // Not a conversation's id: refused, which the keeper leaves unsaid.
        XCTAssertThrowsError(try first.draft(channelID: "general"))
    }

    /// A store over a client's two calls, on a queue of its own as the session's is.
    private func queued(_ client: LumiaClient) -> DraftStore {
        .queued(read: { try client.draft(channelID: $0) }, write: { try client.setDraft(channelID: $0, body: $1, reply: $2) })
    }

    /// Waits for what a field is given next.
    @MainActor
    private func placed(_ field: Field, count: Int) async throws {
        for _ in 0..<200 where field.placed.count < count { try await Task.sleep(for: .milliseconds(25)) }
        XCTAssertGreaterThanOrEqual(field.placed.count, count, "the store did not answer")
    }

    // The keeper over that store, through the queue an app's calls run on:
    // typed, rested, and read by the next client.
    @MainActor
    func testTheKeeperKeepsInTheCoresStore() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let base = URL(string: "http://127.0.0.1:1")!
        let id = "01890000-0000-7000-8000-000000000031"
        let client = try LumiaClient(baseURL: base, dataDirectory: dir)
        let store = queued(client)
        let (pauses, field) = (Pauses(), Field())
        let keeper = DraftKeeper(store: store, wait: pauses.wait)
        keeper.open(id, place: field.place)
        XCTAssertEqual(field.placed, [nil])
        keeper.changed(id, body: "Half a thought", reply: quote("01890000-0000-7000-8000-000000000041"))
        pauses.rest()
        await keeper.settled()
        let kept = DraftSummary(body: "Half a thought", reply: quote("01890000-0000-7000-8000-000000000041"))
        let next = try LumiaClient(baseURL: base, dataDirectory: dir)
        XCTAssertEqual(try next.draft(channelID: id), kept)
        // Another keeper on the same store, as the next launch's is: put in its field.
        let later = Field()
        let launched = DraftKeeper(store: queued(next), wait: pauses.wait)
        launched.open(id, place: later.place)
        try await placed(later, count: 2)
        XCTAssertEqual(later.placed, [nil, kept])
        // Sent: kept while the send is out, let go when it is back.
        keeper.sending(id, body: kept.body, reply: kept.reply)
        keeper.changed(id, body: "", reply: nil)
        await keeper.settled()
        XCTAssertEqual(try next.draft(channelID: id), kept)
        keeper.sent(id)
        await keeper.settled()
        XCTAssertNil(try next.draft(channelID: id))
    }

    /// Another writer on a client's instance store, as the gateway's
    /// connection is while it applies a batch: a process of its own (the
    /// system's `sqlite3`), since a lock is between processes or between
    /// connections of one library, and the core's library is its own.
    private final class OtherWriter {
        private let process = Process()
        private let input = Pipe()
        private let output = Pipe()

        /// Takes the write lock of the `store.sqlite` under `dir` that is not the identity's.
        init(under dir: URL) throws {
            let files = FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil)?.compactMap { $0 as? URL } ?? []
            let store = try XCTUnwrap(files.first { $0.lastPathComponent == "store.sqlite" && !$0.path.contains("/identity/") }, "no instance store under \(dir.path)")
            process.executableURL = URL(fileURLWithPath: "/usr/bin/sqlite3")
            process.arguments = [store.path]
            process.standardInput = input
            process.standardOutput = output
            try process.run()
            try input.fileHandleForWriting.write(contentsOf: Data("BEGIN IMMEDIATE;\nSELECT 'held';\n".utf8))
            let said = String(decoding: output.fileHandleForReading.availableData, as: UTF8.self)
            guard said.contains("held") else { throw XCTSkip("sqlite3 did not take the store's lock: \(said)") }
        }

        func release() throws {
            try input.fileHandleForWriting.write(contentsOf: Data("COMMIT;\n.quit\n".utf8))
            process.waitUntilExit()
        }

        deinit { if process.isRunning { process.terminate() } }
    }

    // The store's write lock is another writer's for a while, as it is the
    // gateway's while it applies what was missed. A draft's write waits for
    // it, up to the store's ten seconds, and nothing on the main actor
    // waits with it: typing, leaving the conversation and going to the
    // background all return at once, and the words are kept once the lock
    // is free. (Made on the main thread, as it was, the same write held it
    // for as long as the lock was held.)
    @MainActor
    func testAWriteThatWaitsForTheStoresLockHoldsNothingUp() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let id = "01890000-0000-7000-8000-000000000031"
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        try client.setDraft(channelID: id, body: "Kept before", reply: nil)
        let (pauses, field) = (Pauses(), Field())
        let keeper = DraftKeeper(store: queued(client), wait: pauses.wait)
        keeper.open(id, place: field.place)
        try await placed(field, count: 2)

        let other = try OtherWriter(under: dir)
        let clock = ContinuousClock()
        let asked = clock.measure {
            keeper.changed(id, body: "Typed while the store was another's", reply: nil)
            pauses.rest()
            keeper.changed(id, body: "Typed while the store was another's, and more", reply: nil)
            keeper.keepNow(id)
            keeper.leave(id)
        }
        XCTAssertLessThan(asked, .milliseconds(200), "keeping a draft waited for the store on the main actor")
        XCTAssertTrue(keeper.writing)
        // Still waiting a second later, and the main actor still free to run this.
        try await Task.sleep(for: .seconds(1))
        XCTAssertTrue(keeper.writing, "the write did not wait for the lock")
        let waited = try await clock.measure {
            try other.release()
            await keeper.settled()
        }
        XCTAssertLessThan(waited, .seconds(5))
        // Both writes landed, in the order they were made.
        XCTAssertEqual(try client.draft(channelID: id)?.body, "Typed while the store was another's, and more")
    }

    // The session's keeper writes while it holds the account, and nothing
    // once it has signed out: the core forgot every draft then, and a
    // screen going away afterwards puts none back.
    @MainActor
    func testNothingIsKeptOnceTheAccountHasLeft() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let id = "01890000-0000-7000-8000-000000000031"
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        let session = LumiaSession(client: client, host: "127.0.0.1:1", signedIn: true, hasIdentity: false)
        // Nothing answers here, so the calls a sign-out makes go nowhere.
        session.ask = { _ in }
        let field = Field()
        let keeper = session.draftKeeper
        keeper.open(id, place: field.place)
        keeper.changed(id, body: "Half a thought", reply: nil)
        keeper.leave(id)
        await keeper.settled()
        XCTAssertEqual(try client.draft(channelID: id)?.body, "Half a thought")

        // One on screen as the account leaves, and one of the session's
        // store made by hand, which forgets nothing by itself.
        keeper.open(id, place: field.place)
        try await placed(field, count: 3)
        keeper.changed(id, body: "Typed as the account left", reply: nil)
        let other = DraftKeeper(store: session.drafts, wait: Pauses().wait)
        other.open(id, place: Field().place)
        other.changed(id, body: "Typed as the account left, elsewhere", reply: nil)
        await session.signOut()
        keeper.leave(id)
        other.leave(id)
        await keeper.settled()
        await other.settled()
        // The core's own sign-out was not run here, so what it had is still
        // there; what matters is the write that did not happen.
        XCTAssertEqual(try client.draft(channelID: id)?.body, "Half a thought")
    }
}
