// A message body as the phone draws it (issue #18): the core's blocks as
// attributed words cut where their look changes, the body as heard, when a
// long one collapses, and the composer's counter.
import CoreGraphics
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class BodiesTests: XCTestCase {
    private func para(_ blocks: [BodyBlock], _ n: Int = 0) -> BodyInline? {
        guard blocks.indices.contains(n), case let .para(inline) = blocks[n] else { return nil }
        return inline
    }

    private func mention(_ id: String, _ start: UInt32, _ end: UInt32) -> MentionSummary {
        MentionSummary(accountId: id, everyone: false, start: start, end: end)
    }

    /// The words of `text` that carry `intent`, in order.
    private func carrying(_ intent: InlinePresentationIntent, in text: AttributedString) -> [String] {
        text.runs[\.inlinePresentationIntent].compactMap { had, range in
            had?.contains(intent) == true ? String(text[range].characters) : nil
        }
    }

    func testUTF16RangesLandAfterAnEmoji() throws {
        // The emoji is two UTF-16 units and one character: the runs start at 3, not 2.
        let inline = try XCTUnwrap(para(parseBody(body: "😀 **bold** `x`", mentions: [])))
        XCTAssertEqual(inline.text, "😀 bold x")
        let all = BodyText.attributed(inline)
        XCTAssertEqual(carrying(.stronglyEmphasized, in: all), ["bold"])
        XCTAssertEqual(carrying(.code, in: all), ["x"])
        let pieces = BodyText.pieces(inline)
        XCTAssertEqual(pieces.map(\.kind), [.words, .code])
        XCTAssertEqual(pieces.map { String($0.text.characters) }, ["😀 bold ", "x"])
        XCTAssertEqual(carrying(.stronglyEmphasized, in: pieces[0].text), ["bold"])
    }

    func testStylesAddUpWhereTheyOverlap() throws {
        let inline = try XCTUnwrap(para(parseBody(body: "***both*** ~~gone~~ *it*", mentions: [])))
        let all = BodyText.attributed(inline)
        XCTAssertEqual(carrying(.stronglyEmphasized, in: all), ["both"])
        XCTAssertEqual(carrying(.emphasized, in: all), ["both", "it"])
        XCTAssertEqual(carrying(.strikethrough, in: all), ["gone"])
    }

    func testOnlyWebLinksAreLinks() throws {
        let inline = try XCTUnwrap(para(parseBody(body: "é [here](https://lumia.chat) and [no](javascript:alert(1))", mentions: [])))
        let all = BodyText.attributed(inline)
        let links = all.runs[\.link].compactMap { url, range in url.map { (String(all[range].characters), $0.absoluteString) } }
        XCTAssertEqual(links.map(\.0), ["here"])
        XCTAssertEqual(links.map(\.1), ["https://lumia.chat"])
        XCTAssertTrue(BodyText.isWeb("HTTPS://lumia.chat"))
        XCTAssertFalse(BodyText.isWeb("lumia-mention:abc"))
    }

    func testAMentionIsAPieceOfItsOwnHeardByName() throws {
        // "hi " is 3 bytes, "@ada" 4: the protocol's ranges are UTF-8 bytes.
        let body = "hi @ada, `@bo` **ok**"
        let mentions = [mention("ada", 3, 7), mention("bo", 10, 13)]
        let blocks = parseBody(body: body, mentions: mentions)
        let inline = try XCTUnwrap(para(blocks))
        let pieces = BodyText.pieces(inline)
        XCTAssertEqual(pieces.map(\.kind), [.words, .mention(0), .words, .code, .words])
        XCTAssertEqual(pieces.map { String($0.text.characters) }, ["hi ", "@ada", ", ", "@bo", " ok"])
        let name: (String) -> String = { $0 == "ada" ? "Ada Lovelace" : "" }
        XCTAssertEqual(BodyText.plain(blocks, mentions: mentions, name: name), "hi @Ada Lovelace, @bo ok")
    }

    func testTheBodyAsHeardKeepsItsBlocksInOrder() {
        let blocks = parseBody(body: "Look:\n\n```\nlet x = 1\n```\n\n> quoted\n\n3. three\n4. four", mentions: [])
        XCTAssertEqual(BodyText.plain(blocks, mentions: [], name: { _ in "" }), "Look:\nlet x = 1\nquoted\n3. three\n4. four")
        XCTAssertEqual(BodyBlocks.codes(blocks), ["let x = 1"])
        XCTAssertFalse(BodyBlocks.editedInline(blocks))
        XCTAssertTrue(BodyBlocks.editedInline(parseBody(body: "```\nx\n```\n\nafter", mentions: [])))
        XCTAssertEqual(BodyBlocks.marker(start: nil, at: 2), "•")
        XCTAssertEqual(BodyBlocks.marker(start: 3, at: 1), "4.")
    }

    func testBlocksStandTheDesktopsRoomApart() {
        let blocks = parseBody(body: "one\n\ntwo\n\n```\ncode\n```\n\nthree", mentions: [])
        XCTAssertEqual((0..<blocks.count).map { BodyBlocks.gap(before: $0, in: blocks) }, [0, 4, 8, 8])
        let item = parseBody(body: "first\n\n- nested", mentions: [])
        XCTAssertEqual(BodyBlocks.gap(before: 1, in: item, inItem: true), 4)
    }

    func testEachCodeBlockIsCopiedByName() {
        XCTAssertEqual(CodeCopy.action(0, of: 1), "Copy code")
        XCTAssertEqual(CodeCopy.action(1, of: 2), "Copy code 2")
    }

    func testALongBodyCollapsesPastFourteenLinesToTen() {
        XCTAssertFalse(BodyCollapse.collapses(lines: 14))
        XCTAssertTrue(BodyCollapse.collapses(lines: 15))
        // A line 22 tall with 4 of it between lines: n lines are 22n - 4.
        let (line, spacing): (CGFloat, CGFloat) = (22, 4)
        XCTAssertEqual(BodyCollapse.lines(height: 18, line: line, spacing: spacing), 1)
        XCTAssertEqual(BodyCollapse.lines(height: 22 * 14 - 4, line: line, spacing: spacing), 14)
        XCTAssertFalse(BodyCollapse.collapses(height: 22 * 14 - 4, line: line, spacing: spacing))
        XCTAssertTrue(BodyCollapse.collapses(height: 22 * 15 - 4, line: line, spacing: spacing))
        XCTAssertEqual(BodyCollapse.height(lines: BodyCollapse.shown, line: line, spacing: spacing), 216)
        XCTAssertEqual(BodyCollapse.lines(height: 0, line: line, spacing: spacing), 0)
        XCTAssertEqual(BodyCollapse.words(expanded: false), "Show more")
        XCTAssertEqual(BodyCollapse.words(expanded: true), "Show less")
        XCTAssertEqual(BodyCollapse.value(expanded: false), "collapsed")
    }

    func testTheCounterSaysWhatIsLeftOrHowMuchTooLong() {
        XCTAssertEqual(MessageCounter.words(left: 1204), "1,204 characters left")
        XCTAssertEqual(MessageCounter.words(left: 1), "1 character left")
        XCTAssertEqual(MessageCounter.words(left: 0), "0 characters left")
        XCTAssertEqual(MessageCounter.words(left: -1), "1 character too long")
        XCTAssertEqual(MessageCounter.words(left: -16384), "16,384 characters too long")
        XCTAssertEqual(MessageCounter.words(left: 1_234_567), "1,234,567 characters left")
        XCTAssertEqual(MessageCounter.words(left: 999), "999 characters left")
    }

    func testGoingOverIsSaidOnce() {
        let under = MessageRoom(shown: true, left: 3)
        let over = MessageRoom(shown: true, left: -1)
        let further = MessageRoom(shown: true, left: -2)
        XCTAssertTrue(MessageCounter.wentOver(from: under, to: over))
        XCTAssertFalse(MessageCounter.wentOver(from: over, to: further))
        XCTAssertFalse(MessageCounter.wentOver(from: over, to: under))
        XCTAssertTrue(MessageCounter.wentOver(from: nil, to: over))
        XCTAssertFalse(MessageCounter.over(nil))
    }

    func testTheCoreCountsTheRoomUnderTheLimit() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        // The protocol's limit until a welcome says otherwise: 16,384 bytes.
        XCTAssertEqual(client.messageRoom("hello"), MessageRoom(shown: false, left: 16379))
        XCTAssertEqual(client.messageRoom(String(repeating: "a", count: 15000)), MessageRoom(shown: true, left: 1384))
        XCTAssertEqual(client.messageRoom(String(repeating: "a", count: 16385)), MessageRoom(shown: true, left: -1))
    }
}
