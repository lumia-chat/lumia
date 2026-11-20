// A server's own emoji on the phone (specs/emoji.md): a body's emoji as
// pieces of their own heard by their present name, the `:` the composer's
// list completes, "Frequently used" kept by server, a chip of one, a
// picture's frames by time, the caret's fallback, and the Emoji section's
// words and refusals.
import CoreGraphics
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class EmojiTests: XCTestCase {
    private func para(_ blocks: [BodyBlock]) -> BodyInline? {
        guard case let .para(inline)? = blocks.first else { return nil }
        return inline
    }

    private func summary(_ id: String, _ name: String, animated: Bool = false, width: UInt32 = 128, height: UInt32 = 128) -> EmojiSummary {
        EmojiSummary(emojiId: id, name: name, imageUrl: "/api/v1/avatars/" + String(repeating: "a", count: 64), animated: animated, width: width, height: height, createdByAccountId: "ada")
    }

    func testAnEmojiIsAPieceOfItsOwnHeardByItsPresentName() throws {
        // "Shipped " is 8 bytes and ":partyparrot:" 13; "é" before ":lgtm:" is 2 bytes.
        let body = "Shipped :partyparrot: é :lgtm: `:hype:`"
        let refs = [EmojiRefSummary(emojiId: "e8", start: 8, end: 21), EmojiRefSummary(emojiId: "e2", start: 25, end: 31)]
        let blocks = parseBodyMarked(body: body, mentions: [], emoji: refs)
        let pieces = BodyText.pieces(try XCTUnwrap(para(blocks)))
        XCTAssertEqual(pieces.map(\.kind), [.words, .emoji(0), .words, .emoji(1), .words, .code])
        XCTAssertEqual(pieces.map { String($0.text.characters) }, ["Shipped ", ":partyparrot:", " é ", ":lgtm:", " ", ":hype:"])
        // The first renamed since (heard by its name now), the second not held (its text as typed).
        let heard = BodyText.plain(blocks, mentions: [], name: { _ in "" }) { $0 == 0 ? ":party_parrot:" : nil }
        XCTAssertEqual(heard, "Shipped :party_parrot: é :lgtm: :hype:")
    }

    func testTheUnderscoreOfANameIsNotEmphasis() throws {
        let body = "a :party_blob: and :party_blob: b"
        let refs = [EmojiRefSummary(emojiId: "e10", start: 2, end: 14), EmojiRefSummary(emojiId: "e10", start: 19, end: 31)]
        let blocks = BodyBlocks.parsed(body, mentions: [], emoji: refs)
        let pieces = BodyText.pieces(try XCTUnwrap(para(blocks)))
        XCTAssertEqual(pieces.map(\.kind), [.words, .emoji(0), .words, .emoji(1), .words])
        XCTAssertEqual(String(pieces[1].text.characters), ":party_blob:")
    }

    func testTheComposersColonWordIsTheOneTheDraftEndsIn() {
        XCTAssertEqual(EmojiQuery.at("Shipped the importer :par"), EmojiQuery(before: "Shipped the importer ", query: "par"))
        XCTAssertEqual(EmojiQuery.at(":pa"), EmojiQuery(before: "", query: "pa"))
        XCTAssertEqual(EmojiQuery.at("x\n:party_b")?.query, "party_b")
        // A time and a face are left alone; so is a colon with nothing after it.
        XCTAssertNil(EmojiQuery.at("at 10:30"))
        XCTAssertNil(EmojiQuery.at("nice :)"))
        XCTAssertNil(EmojiQuery.at("say :"))
        XCTAssertNil(EmojiQuery.at(":par ty"))
        XCTAssertNil(EmojiQuery.at("no colon"))
        XCTAssertEqual(EmojiQuery.at("Shipped :par")?.picking("partyparrot"), "Shipped :partyparrot: ")
        XCTAssertEqual(emojiSearchPrefix(" :par: "), "par")
        XCTAssertEqual(emojiSearchPrefix("::"), "")
    }

    func testAChoiceIsTypedAsItselfOrItsNameAndASpace() {
        XCTAssertEqual(PickedEmoji.unicode("👍").typed, "👍")
        XCTAssertEqual(PickedEmoji.custom(summary("e8", "partyparrot")).typed, ":partyparrot: ")
        XCTAssertEqual(PickedEmoji.custom(summary("e8", "partyparrot")).kept(serverID: "acme"), "emoji:acme:e8")
        XCTAssertEqual(PickedEmoji.unicode("👍").kept(serverID: "acme"), "👍")
    }

    func testFrequentlyUsedHoldsAServersEmojiInThatServerOnly() {
        let recent = ["👀", FrequentEmoji.custom(serverID: "acme", emojiID: "e8"), FrequentEmoji.custom(serverID: "beta", emojiID: "b1"), FrequentEmoji.custom(serverID: "acme", emojiID: "gone")]
        let heldInAcme: (String) -> Bool = { $0 == "e8" }
        let acme = FrequentEmoji.list(recent: recent, serverID: "acme", held: heldInAcme)
        XCTAssertEqual(Array(acme.prefix(2)), ["👀", "emoji:acme:e8"])
        XCTAssertFalse(acme.contains("emoji:beta:b1"))
        XCTAssertFalse(acme.contains("emoji:acme:gone"))
        // In a personal conversation, Unicode's alone; the quick six after.
        let home = FrequentEmoji.list(recent: recent, serverID: "", held: { _ in true })
        XCTAssertEqual(home, ["👀"] + Replies.quick)
        XCTAssertEqual(FrequentEmoji.customParts("emoji:acme:e8")?.emojiID, "e8")
        XCTAssertNil(FrequentEmoji.customParts("👍"))
        XCTAssertNil(FrequentEmoji.customParts("emoji::e8"))
    }

    func testAChipOfARemovedEmojiTakesOnlyThisAccountsBack() {
        let drawing = EmojiDrawing(emojiID: "e8", name: "partyparrot", width: 128, height: 128, animated: true, picture: nil)
        let held = ReactionDisplay(emoji: ":partyparrot:", count: 5, me: false, words: "", emojiID: "e8", custom: drawing)
        XCTAssertEqual(held.id, "e8")
        XCTAssertTrue(held.tappable)
        let removed = ReactionDisplay(emoji: ":oldlogo:", count: 4, me: false, words: "", emojiID: "e0")
        XCTAssertFalse(removed.tappable)
        XCTAssertTrue(ReactionDisplay(emoji: ":oldlogo:", count: 1, me: true, words: "", emojiID: "e0").tappable)
        let unicode = ReactionDisplay(emoji: "🎉", count: 3, me: false, words: "")
        XCTAssertEqual(unicode.id, "🎉")
        XCTAssertTrue(unicode.tappable)
    }

    func testADrawingKeepsItsShape() {
        let wide = EmojiDrawing(emojiID: "e2", name: "lgtm", width: 128, height: 64, animated: false, picture: nil)
        XCTAssertEqual(wide.fitted(in: 32), CGSize(width: 32, height: 16))
        XCTAssertEqual(wide.width(forHeight: 19.5), 39)
        XCTAssertEqual(wide.label, ":lgtm:")
        let tall = EmojiDrawing(emojiID: "e", name: "t", width: 64, height: 128, animated: false, picture: nil)
        XCTAssertEqual(tall.fitted(in: 32), CGSize(width: 16, height: 32))
    }

    private func image() -> CGImage {
        CGContext(data: nil, width: 2, height: 2, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!.makeImage()!
    }

    func testAPlayingPictureShowsTheFrameItsTimeFallsIn() {
        let frames = [image(), image(), image()]
        let picture = EmojiPicture(frames: frames, delays: [0.1, 0.2, 0.1])
        XCTAssertTrue(picture.animated)
        XCTAssertEqual(picture.duration, 0.4, accuracy: 1e-9)
        XCTAssertEqual(picture.shortest, 0.1, accuracy: 1e-9)
        XCTAssertTrue(picture.frame(at: nil) === frames[0], "still under Reduce Motion: the first frame")
        XCTAssertTrue(picture.frame(at: 0.05) === frames[0])
        XCTAssertTrue(picture.frame(at: 0.15) === frames[1])
        XCTAssertTrue(picture.frame(at: 0.35) === frames[2])
        XCTAssertTrue(picture.frame(at: 0.45) === frames[0], "it loops")
        let still = EmojiPicture(frames: [frames[0]], delays: [0])
        XCTAssertFalse(still.animated)
        XCTAssertTrue(still.frame(at: 3) === frames[0])
    }

    func testTheInstancesPicturesDecode() throws {
        // The core's own fixtures: a still PNG and an animated GIF.
        let fixtures = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../../../../core/crates/lumia-client/tests/fixtures").standardized
        let png = try XCTUnwrap(EmojiPicture.decode(Data(contentsOf: fixtures.appendingPathComponent("logo.png"))))
        XCTAssertFalse(png.animated)
        let gif = try XCTUnwrap(EmojiPicture.decode(Data(contentsOf: fixtures.appendingPathComponent("animated.gif"))))
        XCTAssertTrue(gif.animated)
        XCTAssertTrue(gif.delays.allSatisfy { $0 >= 0.02 })
        XCTAssertNil(EmojiPicture.decode(Data("not a picture".utf8)))
        XCTAssertNotNil(EmojiPicture.preview(try Data(contentsOf: fixtures.appendingPathComponent("animated.gif"))))
    }

    @MainActor
    func testAPictureThatCannotBeHadIsItsName() async {
        let pictures = EmojiPictures()
        pictures.fetch = { _ in throw URLError(.notConnectedToInternet) }
        let url = URL(string: "https://chat.acme.dev/api/v1/avatars/" + String(repeating: "b", count: 64))!
        XCTAssertNil(pictures.picture(url))
        await pictures.load(url)
        XCTAssertTrue(pictures.failed(url))
        pictures.keep(only: [])
        XCTAssertFalse(pictures.failed(url), "tried again after the servers' emoji change")
    }

    @MainActor
    func testTheCaretNotKnownPutsAChoiceAtTheEnd() {
        let caret = ComposerCaret()
        XCTAssertEqual(caret.insert(":partyparrot: ", into: "Shipped "), "Shipped :partyparrot: ")
    }

    func testTheSectionsWords() {
        XCTAssertEqual(EmojiWords.count(12, limit: 50), "12 of 50")
        XCTAssertFalse(EmojiWords.atLimit(count: 12, limit: 50))
        XCTAssertTrue(EmojiWords.atLimit(count: 50, limit: 50))
        XCTAssertTrue(EmojiWords.atLimit(count: 51, limit: 50), "a lowered limit keeps what is there and takes no more")
        XCTAssertEqual(EmojiWords.lead(server: "Acme", count: 50, limit: 50), "Acme has as many emoji as it may (50). Remove one to upload another.")
        XCTAssertTrue(EmojiWords.lead(server: "Acme", count: 12, limit: 50).hasPrefix("Used as :name: in Acme’s channels"))
        XCTAssertEqual(EmojiWords.uploaded(by: "Devon Park", animated: true), "Uploaded by Devon Park · Animated")
        XCTAssertEqual(EmojiWords.uploaded(by: "Maya Chen", animated: false), "Uploaded by Maya Chen")
        XCTAssertEqual(EmojiWords.removeTitle("parrotwave"), "Remove :parrotwave:?")
        XCTAssertEqual(EmojiWords.nameHint("acme_heart_v2"), "2 to 32 letters, digits and _. It is typed as :acme_heart_v2: in a message.")
        XCTAssertEqual(EmojiWords.sent(" PartyParrot "), "partyparrot")
        // The core's name from a file, as the form starts from it.
        XCTAssertEqual(emojiNameFromFile(fileName: "Acme-Heart v2.png"), "acme_heart_v2")
    }

    func testRefusalsInTheBoardsWordsWhereTheyWereTried() {
        func said(_ code: String, _ details: [String: String]) -> (EmojiRefusalPlace, String) {
            let r = EmojiRefusal.words(code: code, details: details, message: "raw", server: "Acme", name: "partyparrot")
            return (r.place, r.words)
        }
        XCTAssertTrue(said("ALREADY_EXISTS", ["existing_id": "x"]) == (.name, "Acme already has :partyparrot:. Choose another name."))
        XCTAssertTrue(said("INVALID_ARGUMENT", ["field": "name"]) == (.name, "A name is 2 to 32 of a to z, 0 to 9 and _, with no spaces."))
        XCTAssertTrue(said("INVALID_ARGUMENT", ["field": "picture", "reason": "too_large"]) == (.picture, "This picture is too large for an emoji. An animated one can have at most 200 frames and come to 512 KB once made."))
        XCTAssertTrue(said("INVALID_ARGUMENT", ["field": "picture", "reason": "unsupported"]) == (.picture, "Lumia can’t use this file. Choose a PNG, JPEG, WebP or GIF."))
        XCTAssertTrue(said("INVALID_ARGUMENT", ["field": "picture", "reason": "invalid"]) == (.picture, "This picture couldn’t be read."))
        XCTAssertTrue(said("RESOURCE_EXHAUSTED", ["state": "emoji", "limit": "50"]) == (.form, "Acme has as many emoji as it may (50). Remove one to upload another."))
        XCTAssertTrue(said("PERMISSION_DENIED", [:]) == (.form, "You can’t manage Acme’s emoji."))
        XCTAssertTrue(said("NOT_FOUND", [:]) == (.form, "That emoji has been removed."))
        XCTAssertTrue(said("SOMETHING", [:]) == (.form, "raw"))
        // Before anything is sent: the name's rule, and a picture over 8 MB.
        XCTAssertEqual(EmojiRefusal.before(name: "party-time", bytes: 10)?.words, EmojiWords.nameRule)
        XCTAssertEqual(EmojiRefusal.before(name: "p", bytes: 10)?.words, EmojiWords.nameRule)
        XCTAssertNil(EmojiRefusal.before(name: "Party_Time", bytes: 10), "capitals are lowered, as the instance does")
        XCTAssertEqual(EmojiRefusal.before(name: "fireworks", bytes: 9 * 1024 * 1024)?.place, .picture)
    }
}
