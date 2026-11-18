// What is typed into a composer and not sent (issue #23): a kept draft
// read as its conversation comes on screen, a burst of changes written
// once when it rests, leaving written at once, a send keeping nothing and
// calling off the write that waits, an emptied field, the reply kept with
// the words, a store that fails, an account that has left, and the core's
// store itself read by a second client on the same files.
@testable import LumiaKit
import LumiaFFI
import XCTest

final class DraftsTests: XCTestCase {
    /// A store in memory that keeps as the core's does (blank words keep
    /// nothing), lists its writes and fails when told to.
    @MainActor
    private final class Store {
        var kept: [String: DraftSummary] = [:]
        var writes: [String] = []
        var failing = false

        var calls: DraftStore {
            DraftStore(read: { [self] id in
                if failing { throw LumiaError.Other(detail: "store: unable to open database file") }
                return kept[id]
            }, write: { [self] id, body, reply in
                if failing { throw LumiaError.Other(detail: "store: database is locked") }
                writes.append("\(id): \(body)")
                kept[id] = body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : DraftSummary(body: body, reply: reply)
            })
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

    private func quote(_ id: String = "m1", silent: Bool = false) -> QuoteSummary {
        Replies.quote(messageID: id, authorID: "ada", body: "Shall we ship on Friday?", silent: silent)
    }

    @MainActor
    func testAKeptDraftIsReadAsItsConversationComesOnScreen() {
        let (store, pauses) = (Store(), Pauses())
        let kept = DraftSummary(body: "Half a thought", reply: quote())
        store.kept["design"] = kept
        let keeper = DraftKeeper(wait: pauses.wait)
        XCTAssertNil(keeper.channelID)
        XCTAssertEqual(keeper.open("design", in: store.calls), kept)
        XCTAssertEqual(keeper.channelID, "design")
        // The screen puts it in its field and reply bar, which is a change
        // of what the composer holds and nothing to write.
        keeper.changed(body: kept.body, reply: kept.reply)
        XCTAssertEqual(pauses.waiting, 0)
        keeper.leave()
        XCTAssertEqual(store.writes, [])
        // Another conversation has its own, or none: the first one's words do not follow there.
        XCTAssertNil(keeper.open("random", in: store.calls))
        XCTAssertEqual(store.writes, [])
        XCTAssertEqual(store.kept["design"], kept)
    }

    @MainActor
    func testABurstOfChangesIsOneWriteOnceItRests() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(wait: pauses.wait)
        _ = keeper.open("design", in: store.calls)
        for words in ["H", "Ha", "Half", "Half a thought"] { keeper.changed(body: words, reply: nil) }
        // Each change calls off the wait before it: one is left, and nothing is written yet.
        XCTAssertEqual(pauses.waiting, 1)
        XCTAssertEqual(pauses.asked, Array(repeating: .milliseconds(400), count: 4))
        XCTAssertEqual(store.writes, [])
        pauses.rest()
        XCTAssertEqual(store.writes, ["design: Half a thought"])
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "Half a thought", reply: nil))
        // Rested and kept: nothing more to write, now or on leaving.
        pauses.rest()
        keeper.leave()
        XCTAssertEqual(store.writes.count, 1)
        // A key after that is the next burst.
        keeper.changed(body: "Half a thought.", reply: nil)
        pauses.rest()
        XCTAssertEqual(store.writes, ["design: Half a thought", "design: Half a thought."])
    }

    @MainActor
    func testLeavingKeepsAtOnce() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(wait: pauses.wait)
        _ = keeper.open("design", in: store.calls)
        keeper.changed(body: "Not rested yet", reply: nil)
        // The screen goes, or the app leaves the foreground: written now, with nothing left waiting.
        keeper.leave()
        XCTAssertEqual(store.writes, ["design: Not rested yet"])
        XCTAssertEqual(pauses.waiting, 0)
        pauses.rest()
        keeper.leave()
        XCTAssertEqual(store.writes.count, 1)

        // Another conversation in the same screen: the first one's words are
        // kept under its own name before the next is read.
        keeper.changed(body: "Not rested yet, still", reply: nil)
        XCTAssertNil(keeper.open("random", in: store.calls))
        XCTAssertEqual(store.writes, ["design: Not rested yet", "design: Not rested yet, still"])
        XCTAssertEqual(pauses.waiting, 0)
        keeper.changed(body: "Elsewhere", reply: nil)
        keeper.leave()
        XCTAssertEqual(store.kept["random"]?.body, "Elsewhere")
        XCTAssertEqual(store.kept["design"]?.body, "Not rested yet, still")
        XCTAssertEqual(keeper.open("design", in: store.calls)?.body, "Not rested yet, still")
    }

    @MainActor
    func testASendKeepsNothingAndCallsOffTheWriteThatWaits() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(wait: pauses.wait)
        _ = keeper.open("design", in: store.calls)
        keeper.changed(body: "Shipping on Friday", reply: quote())
        pauses.rest()
        XCTAssertNotNil(store.kept["design"])
        // More typed, then sent before it rested.
        keeper.changed(body: "Shipping on Friday.", reply: quote())
        XCTAssertEqual(pauses.waiting, 1)
        keeper.sent()
        XCTAssertNil(store.kept["design"], "a send keeps nothing, at once")
        XCTAssertEqual(pauses.waiting, 0, "the write that waited was called off")
        // The pause running out brings nothing back, nor does the field the
        // send emptied, nor leaving.
        pauses.rest()
        keeper.changed(body: "", reply: nil)
        XCTAssertEqual(pauses.waiting, 0)
        keeper.leave()
        XCTAssertNil(store.kept["design"])
        XCTAssertEqual(store.writes, ["design: Shipping on Friday", "design: "])

        // The send failed and the screen put the words back: kept again.
        keeper.changed(body: "Shipping on Friday.", reply: quote())
        pauses.rest()
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "Shipping on Friday.", reply: quote()))

        // Sent within the pause of the first key: nothing was ever written, and nothing is.
        _ = keeper.open("random", in: store.calls)
        keeper.changed(body: "ok", reply: nil)
        keeper.sent()
        pauses.rest()
        XCTAssertNil(store.kept["random"])
    }

    @MainActor
    func testAnEmptiedFieldKeepsNothing() {
        let (store, pauses) = (Store(), Pauses())
        store.kept["design"] = DraftSummary(body: "Half a thought", reply: quote())
        let keeper = DraftKeeper(wait: pauses.wait)
        _ = keeper.open("design", in: store.calls)
        // All blank is empty, and its reply is not kept without words.
        keeper.changed(body: " \n", reply: quote())
        pauses.rest()
        XCTAssertNil(store.kept["design"])
        XCTAssertEqual(store.writes.count, 1)
        // A reply chosen before a word is typed is nothing to keep yet.
        keeper.changed(body: "", reply: quote("m2"))
        XCTAssertEqual(pauses.waiting, 0)
        keeper.leave()
        XCTAssertEqual(store.writes.count, 1)
    }

    @MainActor
    func testTheReplyIsKeptWithTheWords() {
        let (store, pauses) = (Store(), Pauses())
        let keeper = DraftKeeper(wait: pauses.wait)
        _ = keeper.open("design", in: store.calls)
        keeper.changed(body: "On it", reply: nil)
        pauses.rest()
        // The reply alone is a change: begun, its "@ On" turned off, cancelled.
        keeper.changed(body: "On it", reply: quote())
        pauses.rest()
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "On it", reply: quote()))
        keeper.changed(body: "On it", reply: quote(silent: true))
        pauses.rest()
        XCTAssertEqual(store.kept["design"]?.reply?.silent, true)
        keeper.changed(body: "On it", reply: nil)
        keeper.leave()
        XCTAssertEqual(store.kept["design"], DraftSummary(body: "On it", reply: nil))
        XCTAssertEqual(store.writes.count, 4)
    }

    @MainActor
    func testAStoreThatFailsIsSilentAndAskedAgain() {
        let (store, pauses) = (Store(), Pauses())
        store.kept["design"] = DraftSummary(body: "Kept before", reply: nil)
        store.failing = true
        let keeper = DraftKeeper(wait: pauses.wait)
        // Not read: the field opens empty, as it did before there were drafts.
        XCTAssertNil(keeper.open("design", in: store.calls))
        keeper.changed(body: "Typed anyway", reply: nil)
        pauses.rest()
        keeper.sent()
        keeper.changed(body: "And again", reply: nil)
        pauses.rest()
        XCTAssertEqual(store.writes, [])
        // What could not be written is written when the store next takes it.
        store.failing = false
        keeper.leave()
        XCTAssertEqual(store.kept["design"]?.body, "And again")
    }

    @MainActor
    func testNothingIsToldBeforeAConversationIsOnScreen() {
        let pauses = Pauses()
        let keeper = DraftKeeper(wait: pauses.wait)
        keeper.changed(body: "Nowhere", reply: nil)
        keeper.sent()
        keeper.leave()
        XCTAssertEqual(pauses.waiting, 0)
    }

    // The wait an app uses is a sleep: what is typed is in the store soon
    // after it rests, and one called off writes nothing.
    @MainActor
    func testTheRestedWriteIsASleep() async throws {
        let store = Store()
        let keeper = DraftKeeper()
        _ = keeper.open("design", in: store.calls)
        keeper.changed(body: "Half", reply: nil)
        keeper.changed(body: "Half a thought", reply: nil)
        XCTAssertEqual(store.writes, [])
        for _ in 0..<100 where store.writes.isEmpty { try await Task.sleep(for: .milliseconds(50)) }
        XCTAssertEqual(store.writes, ["design: Half a thought"])
        keeper.changed(body: "Half a thought, sent", reply: nil)
        keeper.sent()
        try await Task.sleep(for: DraftKeeper.rest * 2)
        XCTAssertEqual(store.writes, ["design: Half a thought", "design: "])
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

    // The keeper over that store: typed, rested, and read by the next client.
    @MainActor
    func testTheKeeperKeepsInTheCoresStore() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let base = URL(string: "http://127.0.0.1:1")!
        let id = "01890000-0000-7000-8000-000000000031"
        let client = try LumiaClient(baseURL: base, dataDirectory: dir)
        let store = DraftStore(read: { try client.draft(channelID: $0) }, write: { try client.setDraft(channelID: $0, body: $1, reply: $2) })
        let pauses = Pauses()
        let keeper = DraftKeeper(wait: pauses.wait)
        XCTAssertNil(keeper.open(id, in: store))
        keeper.changed(body: "Half a thought", reply: quote("01890000-0000-7000-8000-000000000041"))
        pauses.rest()
        let next = try LumiaClient(baseURL: base, dataDirectory: dir)
        XCTAssertEqual(try next.draft(channelID: id), DraftSummary(body: "Half a thought", reply: quote("01890000-0000-7000-8000-000000000041")))
        XCTAssertEqual(DraftKeeper(wait: pauses.wait).open(id, in: store)?.body, "Half a thought")
        keeper.sent()
        XCTAssertNil(try next.draft(channelID: id))
    }

    // The session's store writes while it holds the account, and nothing
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
        let keeper = DraftKeeper(wait: Pauses().wait)
        XCTAssertNil(keeper.open(id, in: session.drafts))
        keeper.changed(body: "Half a thought", reply: nil)
        keeper.leave()
        XCTAssertEqual(try session.drafts.read(id)?.body, "Half a thought")

        keeper.changed(body: "Typed as the account left", reply: nil)
        await session.signOut()
        keeper.leave()
        // The core's own sign-out was not run here, so what it had is still
        // there; what matters is the write that did not happen.
        XCTAssertEqual(try client.draft(channelID: id)?.body, "Half a thought")
    }
}
