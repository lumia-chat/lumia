// A conversation's list before it is drawn: blocks, days, and who is
// typing, by the rules the desktop shell's tests hold it to
// (apps/desktop/tests/shell.rs).
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class TimelineTests: XCTestCase {
    private let minute: Int64 = 60_000_000
    private let day: Int64 = 86_400_000_000

    /// A calendar of its own, so the tests read the same wherever they run.
    private var calendar: Calendar {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "America/Vancouver")!
        return c
    }

    private func micros(_ year: Int, _ month: Int, _ day: Int, _ hour: Int, _ minute: Int) -> Int64 {
        let date = calendar.date(from: DateComponents(year: year, month: month, day: day, hour: hour, minute: minute))!
        return Int64(date.timeIntervalSince1970 * 1_000_000)
    }

    // MARK: Blocks

    func testMessagesOnePersonSendsCloseTogetherAreOneBlock() {
        let noon = micros(2026, 11, 12, 12, 0)
        let stamps = [
            MessageStamp(authorID: "grace", atMicros: noon),
            MessageStamp(authorID: "grace", atMicros: noon + minute),
            // More than five minutes after the one before it: a block of its own.
            MessageStamp(authorID: "grace", atMicros: noon + 7 * minute),
            MessageStamp(authorID: "me", atMicros: noon + 8 * minute),
            MessageStamp(authorID: "me", atMicros: noon + 9 * minute),
            // The same person a day on: under the day's divider, so not in yesterday's block.
            MessageStamp(authorID: "me", atMicros: noon + 9 * minute + day),
            MessageStamp(authorID: "me", atMicros: noon + 10 * minute + day),
        ]
        let places = MessageBlocks.places(stamps, calendar: calendar)
        XCTAssertEqual(places.map(\.opens), [true, false, true, true, false, true, false], "who is named: the first message of each block")
        XCTAssertEqual(places.map(\.closes), [false, true, true, false, true, false, true], "where the room between blocks goes")
        XCTAssertEqual(places.map(\.startsDay), [true, false, false, false, false, true, false], "which carry a day's divider")
    }

    func testABlockIsMeasuredFromTheMessageBefore() {
        let noon = micros(2026, 11, 12, 12, 0)
        // Four minutes apart each: twelve minutes from first to last, and still one block.
        let run = (0..<4).map { MessageStamp(authorID: "grace", atMicros: noon + Int64($0) * 4 * minute) }
        XCTAssertEqual(MessageBlocks.places(run, calendar: calendar).map(\.opens), [true, false, false, false])

        let before = MessageStamp(authorID: "grace", atMicros: noon)
        let just = MessageStamp(authorID: "grace", atMicros: noon + 5 * minute - 1)
        let late = MessageStamp(authorID: "grace", atMicros: noon + 5 * minute)
        XCTAssertTrue(MessageBlocks.joins(before, just, calendar: calendar))
        XCTAssertFalse(MessageBlocks.joins(before, late, calendar: calendar), "five minutes is a pause")
        // Either way round: two devices' clocks stamp a conversation's messages.
        XCTAssertTrue(MessageBlocks.joins(just, before, calendar: calendar))
        XCTAssertFalse(MessageBlocks.joins(late, before, calendar: calendar))
    }

    func testABlockNeedsTheSameKnownPersonOnTheSameDay() {
        let noon = micros(2026, 11, 12, 12, 0)
        let grace = MessageStamp(authorID: "grace", atMicros: noon)
        XCTAssertFalse(MessageBlocks.joins(grace, MessageStamp(authorID: "alan", atMicros: noon + minute), calendar: calendar))
        // Nobody known wrote either: not one person's block.
        let nobody = MessageStamp(authorID: "", atMicros: noon)
        XCTAssertFalse(MessageBlocks.joins(nobody, MessageStamp(authorID: "", atMicros: noon + minute), calendar: calendar))

        // Three minutes apart across midnight: the second opens the new day.
        let late = MessageStamp(authorID: "grace", atMicros: micros(2026, 11, 12, 23, 58))
        let early = MessageStamp(authorID: "grace", atMicros: micros(2026, 11, 13, 0, 1))
        XCTAssertFalse(MessageBlocks.joins(late, early, calendar: calendar))
        XCTAssertEqual(
            MessageBlocks.places([late, early], calendar: calendar),
            [MessagePlace(opens: true, closes: true, startsDay: true), MessagePlace(opens: true, closes: true, startsDay: true)]
        )
        // The day is this device's: the same two moments are one evening further west.
        var honolulu = calendar
        honolulu.timeZone = TimeZone(identifier: "Pacific/Honolulu")!
        XCTAssertTrue(MessageBlocks.joins(late, early, calendar: honolulu))
    }

    func testNoMessagesAndOne() {
        XCTAssertEqual(MessageBlocks.places([], calendar: calendar), [])
        XCTAssertEqual(
            MessageBlocks.places([MessageStamp(authorID: "grace", atMicros: 0)], calendar: calendar),
            [MessagePlace(opens: true, closes: true, startsDay: true)]
        )
    }

    func testARowSaysWhoseItIsAndWhen() {
        XCTAssertEqual(LumiaMessageRow.heading(author: "Grace Hopper", time: "12:01 PM"), "Grace Hopper, 12:01 PM")
    }

    func testDaysAreNamed() {
        let locale = Locale(identifier: "en_US")
        let now = calendar.date(from: DateComponents(year: 2026, month: 11, day: 12, hour: 9))!
        let label = { (micros: Int64) in dayLabel(ofMicros: micros, now: now, calendar: self.calendar, locale: locale) }
        XCTAssertEqual(label(micros(2026, 11, 12, 0, 0)), "Today")
        XCTAssertEqual(label(micros(2026, 11, 12, 23, 59)), "Today")
        XCTAssertEqual(label(micros(2026, 11, 11, 23, 59)), "Yesterday")
        XCTAssertEqual(label(micros(2026, 11, 10, 12, 0)), "November 10")
        XCTAssertEqual(label(micros(2025, 12, 31, 12, 0)), "December 31, 2025")
    }

    // MARK: Typing

    func testWhoIsTypingIsKeptInTheOrderTheyBegan() {
        var typing = TypingRoster()
        typing.began(channelID: "general", accountID: "grace", untilMicros: 8_000_000, me: "me")
        typing.began(channelID: "general", accountID: "alan", untilMicros: 9_000_000, me: "me")
        typing.began(channelID: "random", accountID: "ada", untilMicros: 7_000_000, me: "me")
        // Never this account, whatever the instance sends.
        typing.began(channelID: "general", accountID: "me", untilMicros: 9_000_000, me: "me")
        XCTAssertEqual(typing.accountIDs(in: "general"), ["grace", "alan"])
        XCTAssertEqual(typing.accountIDs(in: "random"), ["ada"])
        XCTAssertEqual(typing.accountIDs(in: "elsewhere"), [])
        XCTAssertEqual(typing.nextEnd, 7_000_000)

        // A later word from the same account only moves its end: no second entry, the same place.
        typing.began(channelID: "general", accountID: "grace", untilMicros: 12_000_000, me: "me")
        XCTAssertEqual(typing.accountIDs(in: "general"), ["grace", "alan"])

        // Silence ends it when its time runs out, and not before.
        XCTAssertFalse(typing.expire(nowMicros: 6_999_999))
        XCTAssertTrue(typing.expire(nowMicros: 7_000_000))
        XCTAssertEqual(typing.accountIDs(in: "random"), [])
        XCTAssertEqual(typing.nextEnd, 9_000_000)
        XCTAssertTrue(typing.expire(nowMicros: 9_000_000))
        XCTAssertEqual(typing.accountIDs(in: "general"), ["grace"])

        // A message from someone ends theirs at once, in that channel only.
        typing.began(channelID: "random", accountID: "grace", untilMicros: 12_000_000, me: "me")
        typing.stopped(channelID: "general", accountID: "grace")
        XCTAssertEqual(typing.accountIDs(in: "general"), [])
        XCTAssertEqual(typing.accountIDs(in: "random"), ["grace"])
        typing.stopped(channelID: "general", accountID: "nobody")

        typing.clear()
        XCTAssertEqual(typing, TypingRoster())
        XCTAssertNil(typing.nextEnd)
    }

    func testWhatTheTypingLineSays() {
        XCTAssertNil(typingWords([]))
        let one = typingWords(["Grace Hopper"])
        XCTAssertEqual(one?.text, "Grace Hopper is typing…")
        XCTAssertEqual(one?.names, "Grace Hopper")
        let two = typingWords(["Grace Hopper", "Alan Turing"])
        XCTAssertEqual(two?.text, "Grace Hopper and Alan Turing are typing…")
        XCTAssertEqual(two?.names, "Grace Hopper and Alan Turing")
        let three = typingWords(["Grace Hopper", "Alan Turing", "Ada Lovelace"])
        XCTAssertEqual(three?.text, "Several people are typing…")
        XCTAssertEqual(three?.names, "")
        XCTAssertEqual(typingWords(["a", "b", "c", "d"]), three)
    }

    func testTheDotsBrightenInTurn() {
        let at = { (dot: Int, seconds: Double) in TypingDots.opacity(dot: dot, at: seconds) }
        // The boards' keyframes: 0.3 at the start, 1 at 40% of 1.2 s, 0.3 from 80% on.
        XCTAssertEqual(at(0, 0), 0.3, accuracy: 0.001)
        XCTAssertEqual(at(0, 0.48), 1, accuracy: 0.001)
        XCTAssertEqual(at(0, 0.96), 0.3, accuracy: 0.001)
        XCTAssertEqual(at(0, 1.1), 0.3, accuracy: 0.001)
        XCTAssertEqual(at(0, 1.2 + 0.48), 1, accuracy: 0.001, "and round again")
        // Halfway up is halfway bright: the easing is symmetrical.
        XCTAssertEqual(at(0, 0.24), 0.65, accuracy: 0.001)
        // The second and third are 0.15 s behind the one before.
        XCTAssertEqual(at(1, 0.48 + 0.15), 1, accuracy: 0.001)
        XCTAssertEqual(at(2, 0.48 + 0.3), 1, accuracy: 0.001)
        XCTAssertEqual(at(2, 0), at(0, 0.9), accuracy: 0.001)
        for step in 0..<240 {
            let seconds = Double(step) / 100
            for dot in 0..<TypingDots.count {
                XCTAssertTrue((0.2999...1.0001).contains(at(dot, seconds)), "dot \(dot) at \(seconds)")
            }
        }
    }

    // MARK: The session

    @MainActor
    private func session() async throws -> (LumiaSession, URL) {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        return (await LumiaSession(client: client), dir)
    }

    private func message(by author: String, in channel: String) -> EventSummary {
        EventSummary(seq: 1, kind: "MESSAGE_CREATED", message: MessageSummary(messageId: "m1", channelId: channel, seq: 1, authorAccountId: author, body: "Done.", createdAtMicros: 0, deleted: false))
    }

    @MainActor
    func testTheSessionKeepsWhoIsTypingAndAMessageEndsIt() async throws {
        let (session, dir) = try await session()
        defer { try? FileManager.default.removeItem(at: dir) }
        let later = session.nowMicros() + 60_000_000
        await session.handle(.typing(channelId: "general", accountId: "grace", untilMicros: later))
        await session.handle(.typing(channelId: "general", accountId: "alan", untilMicros: later))
        XCTAssertEqual(session.typing.accountIDs(in: "general"), ["grace", "alan"])
        // Nobody here knows them: a name is the start of the id until a profile says more.
        XCTAssertEqual(session.typingNames(in: "general"), ["grace", "alan"])

        // Someone else's message leaves grace typing; her own ends it at once.
        await session.handle(.applied(channelId: "general", events: [message(by: "ada", in: "general")]))
        XCTAssertEqual(session.typing.accountIDs(in: "general"), ["grace", "alan"])
        await session.handle(.applied(channelId: "general", events: [message(by: "grace", in: "general")]))
        XCTAssertEqual(session.typing.accountIDs(in: "general"), ["alan"])

        // Signing out forgets everyone.
        await session.signOut()
        XCTAssertEqual(session.typing, TypingRoster())
    }

    @MainActor
    func testSilenceEndsTypingWhenItsTimeRunsOut() async throws {
        let (session, dir) = try await session()
        defer { try? FileManager.default.removeItem(at: dir) }
        let now = session.nowMicros()
        await session.handle(.typing(channelId: "general", accountId: "grace", untilMicros: now + 150_000))
        await session.handle(.typing(channelId: "general", accountId: "alan", untilMicros: now + 60_000_000))
        XCTAssertEqual(session.typing.accountIDs(in: "general"), ["grace", "alan"])
        // Nothing arrives to say grace stopped: the session wakes itself for it.
        for _ in 0..<100 where session.typing.accountIDs(in: "general").count == 2 {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertEqual(session.typing.accountIDs(in: "general"), ["alan"])
        XCTAssertGreaterThanOrEqual(session.nowMicros(), now + 150_000, "not before her time ran out")

        // A later word moves the end: alan, about to run out, is still there after his first end.
        let soon = session.nowMicros() + 100_000
        await session.handle(.typing(channelId: "general", accountId: "alan", untilMicros: soon))
        await session.handle(.typing(channelId: "general", accountId: "alan", untilMicros: soon + 60_000_000))
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(session.typing.accountIDs(in: "general"), ["alan"])
        await session.signOut()
    }
}
