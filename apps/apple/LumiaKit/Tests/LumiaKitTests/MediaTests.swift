// Attachments and link cards (specs/media.md) as the phone decides them:
// the composer's strip and when Send goes, the link previews while
// composing, how pictures sit in a column, the viewer's zoom as its scroll
// view carries out the core's (the cases of
// `proto/vectors/viewer/cases.json` a phone has), the words said of each, an edit
// that takes media off, and what the platform decodes before the core
// makes an attachment (a picture upright and unpremultiplied, a GIF left
// to the core, the bounds before decoding, a video's poster, length and
// turned size).
import AVFoundation
import CoreGraphics
import Foundation
import ImageIO
@testable import LumiaKit
import LumiaFFI
import UniformTypeIdentifiers
import XCTest

final class MediaTests: XCTestCase {
    private func draft(_ id: String, size: UInt64 = 1000) -> AttachmentDraft {
        AttachmentDraft(draftId: id, channelId: "c", kind: "image", name: "\(id).jpg", mediaType: "image/jpeg", width: 4, height: 3, durationMs: 0, placeholder: Data(), alt: "", sizeBytes: size)
    }

    private func card(_ url: String, title: String = "Title") -> CardSummary {
        CardSummary(url: url, title: title, description: "", siteName: "Site", host: URL(string: url)?.host ?? "", color: "", imageUrl: "", image: nil, imageWidth: 0, imageHeight: 0, video: false)
    }

    private func attachment(_ id: String, kind: String = "image", expired: Bool = false) -> AttachmentSummary {
        let blob = BlobSummary(blobId: "b\(id)", sizeBytes: 10, key: Data(), nonce: Data(), hash: Data())
        return AttachmentSummary(attachmentId: id, kind: kind, name: "\(id).jpg", mediaType: "image/jpeg", width: 4, height: 3, durationMs: 0, placeholder: Data(), alt: "", sizeBytes: 10, main: blob, thumbnail: nil, expired: expired)
    }

    // MARK: The composer's strip

    func testSendWaitsForEveryUploadAndCarriesTheDraftsInOrder() throws {
        var strip = ComposerAttachments()
        XCTAssertFalse(strip.canSend(body: "  "), "nothing to send")
        XCTAssertTrue(strip.canSend(body: "hi"))
        let a = try XCTUnwrap(strip.add(id: "a", name: "Photo.heic"))
        let b = try XCTUnwrap(strip.add(id: "b", name: "b.pdf"))
        XCTAssertTrue(strip.uploading)
        XCTAssertEqual(strip.items[0].shownName, "Photo.heic", "the picker's stand-in until the core names it")
        XCTAssertFalse(strip.canSend(body: "hi"), "a send waits while a file is on its way")
        strip.uploaded(b, draft("B"))
        XCTAssertFalse(strip.canSend(body: ""), "a is still uploading")
        strip.uploaded(a, draft("A"))
        XCTAssertTrue(strip.canSend(body: ""), "files go with no words")
        // In the order they were added, not the order they finished.
        XCTAssertEqual(strip.draftIDs, ["A", "B"])
        XCTAssertEqual(strip.items[0].shownName, "A.jpg")
        XCTAssertEqual(MediaWords.state(strip.items[0]), MediaWords.size(1000))
    }

    func testARefusedFileSaysWhyAndStaysBehindASend() throws {
        var strip = ComposerAttachments()
        let a = try XCTUnwrap(strip.add(id: "a", name: "a.jpg"))
        let b = try XCTUnwrap(strip.add(id: "b", name: "huge.mov"))
        strip.uploaded(a, draft("A"))
        strip.failed(b, "Too large")
        XCTAssertFalse(strip.uploading)
        XCTAssertEqual(strip.items[1].failure, "Too large")
        XCTAssertEqual(MediaWords.state(strip.items[1]), "Too large")
        XCTAssertEqual(strip.draftIDs, ["A"])
        strip.sent()
        XCTAssertEqual(strip.items.map(\.id), ["b"], "the refused one stays, saying why")
        XCTAssertNil(strip.remove("b"), "nothing to discard for a file never uploaded")
        XCTAssertTrue(strip.isEmpty)
    }

    func testTheXDiscardsItsDraftAndAnAnswerAfterItChangesNothing() throws {
        var strip = ComposerAttachments()
        let a = try XCTUnwrap(strip.add(id: "a", name: "a.jpg"))
        strip.uploaded(a, draft("A"))
        XCTAssertEqual(strip.remove(a), "A")
        // An upload that comes back after its × finds nothing to fill.
        let b = try XCTUnwrap(strip.add(id: "b", name: "b.jpg"))
        _ = strip.remove(b)
        strip.uploaded(b, draft("B"))
        strip.failed(b, "late")
        XCTAssertTrue(strip.isEmpty)
    }

    func testAMessageCarriesAtMostTen() {
        var strip = ComposerAttachments()
        for n in 0..<10 { XCTAssertNotNil(strip.add(id: "\(n)", name: "\(n)")) }
        XCTAssertEqual(strip.room, 0)
        XCTAssertNil(strip.add(name: "eleven"))
        XCTAssertEqual(strip.items.count, ComposerAttachments.most)
    }

    func testAltTextAndClearing() throws {
        var strip = ComposerAttachments()
        let a = try XCTUnwrap(strip.add(id: "a", name: "a.jpg"))
        strip.uploaded(a, draft("A"))
        var described = draft("A")
        described.alt = "A dog on a beach"
        strip.updated(described)
        XCTAssertEqual(strip.items[0].draft?.alt, "A dog on a beach")
        XCTAssertEqual(strip.clear(), ["A"])
        XCTAssertTrue(strip.isEmpty)
    }

    // MARK: Link previews

    func testEachLinkIsAskedForOnceAndShownInTheOrderItAppears() {
        var previews = LinkPreviews()
        XCTAssertEqual(previews.update(links: ["https://a.dev", "https://b.dev"]), ["https://a.dev", "https://b.dev"])
        // Typing on with the same links asks for nothing while they are on their way.
        XCTAssertEqual(previews.update(links: ["https://a.dev", "https://b.dev"]), [])
        previews.arrived("https://b.dev", card: card("https://b.dev", title: "B"))
        previews.arrived("https://a.dev", card: card("https://a.dev", title: "A"))
        XCTAssertEqual(previews.shown.map(\.title), ["A", "B"])
        // Nor once they have come back, card or none.
        XCTAssertEqual(previews.update(links: ["https://b.dev", "https://a.dev", "https://c.dev"]), ["https://c.dev"])
        previews.arrived("https://c.dev", card: nil)
        XCTAssertEqual(previews.shown.map(\.title), ["B", "A"], "in the body's order now, and c has none")
    }

    func testALinkTakenOutOfTheDraftLosesItsCardAndComesBackWithoutAsking() {
        var previews = LinkPreviews()
        _ = previews.update(links: ["https://a.dev"])
        previews.arrived("https://a.dev", card: card("https://a.dev"))
        XCTAssertEqual(previews.update(links: []), [])
        XCTAssertTrue(previews.shown.isEmpty)
        XCTAssertEqual(previews.update(links: ["https://a.dev"]), [], "its answer is kept")
        XCTAssertEqual(previews.shown.count, 1)
    }

    func testTheXHidesACardForTheDraftAndASendKeepsOnlyThoseShown() {
        var previews = LinkPreviews()
        _ = previews.update(links: ["https://a.dev", "https://b.dev"])
        previews.arrived("https://a.dev", card: card("https://a.dev"))
        previews.arrived("https://b.dev", card: card("https://b.dev"))
        previews.dismiss("https://a.dev")
        XCTAssertEqual(previews.shown.map(\.url), ["https://b.dev"])
        XCTAssertEqual(previews.update(links: ["https://a.dev", "https://b.dev"]), [], "a card × is not asked for again")
        XCTAssertEqual(previews.kept(in: ["https://a.dev", "https://b.dev"]), ["https://b.dev"])
        // A card whose link the body no longer holds is not kept.
        XCTAssertEqual(previews.kept(in: ["https://a.dev"]), [])
        previews.reset()
        XCTAssertEqual(previews, LinkPreviews())
    }

    func testAtMostThreeLinksAndAPauseOfHalfASecond() {
        var previews = LinkPreviews()
        let links = (1...5).map { "https://\($0).dev" }
        XCTAssertEqual(previews.update(links: links), Array(links.prefix(3)))
        XCTAssertEqual(LinkPreviews.pause, .milliseconds(500))
    }

    func testTheCoreSaysWhichLinksMayHaveCards() {
        XCTAssertEqual(cardLinks(body: "see https://example.com/a and http://b.dev"), ["https://example.com/a", "http://b.dev"])
        XCTAssertEqual(cardLinks(body: "no card for <https://example.com> or `https://x.dev`"), [])
    }

    // MARK: Layout

    func testOnePictureKeepsItsShapeWithinTheColumnAndFourHundredHigh() {
        // Wide: the column's width.
        XCTAssertEqual(MediaLayout.single(width: 2560, height: 1440, column: 300), CGSize(width: 300, height: 168))
        // Tall: 400 high, narrower.
        XCTAssertEqual(MediaLayout.single(width: 1000, height: 2000, column: 300), CGSize(width: 200, height: 400))
        // Small: no wider than its own pixels.
        XCTAssertEqual(MediaLayout.single(width: 120, height: 60, column: 300), CGSize(width: 120, height: 60))
        // Unknown: the column's width at 4:3.
        XCTAssertEqual(MediaLayout.single(width: 0, height: 0, column: 300), CGSize(width: 300, height: 225))
        XCTAssertEqual(MediaLayout.single(width: 10, height: 10, column: 0), .zero)
    }

    func testSeveralPicturesAreAGridOfSquares() {
        XCTAssertEqual(MediaLayout.grid(count: 2, column: 302, gap: 2).columns, 2)
        XCTAssertEqual(MediaLayout.grid(count: 2, column: 302, gap: 2).side, 150)
        XCTAssertEqual(MediaLayout.grid(count: 3, column: 304, gap: 2).columns, 3)
        XCTAssertEqual(MediaLayout.grid(count: 3, column: 304, gap: 2).side, 100)
        XCTAssertEqual(MediaLayout.grid(count: 4, column: 300, gap: 2).columns, 2)
        XCTAssertEqual(MediaLayout.grid(count: 4, column: 300, gap: 2).rows, 2)
        XCTAssertEqual(MediaLayout.grid(count: 10, column: 300, gap: 2).rows, 4)
        let tiles = MediaLayout.tiles(count: 5, column: 304, gap: 2)
        XCTAssertEqual(tiles.map(\.origin), [CGPoint(x: 0, y: 0), CGPoint(x: 102, y: 0), CGPoint(x: 204, y: 0), CGPoint(x: 0, y: 102), CGPoint(x: 102, y: 102)])
        XCTAssertEqual(MediaLayout.grid(count: 1, column: 300, gap: 2).columns, 1)
    }

    // MARK: The viewer

    /// The viewer's cases every client agrees on, `proto/vectors/viewer/cases.json`.
    private func viewerCases() throws -> [String: Any] {
        var root = URL(fileURLWithPath: #filePath)
        for _ in 0..<6 { root.deleteLastPathComponent() }
        let data = try Data(contentsOf: root.appendingPathComponent("proto/vectors/viewer/cases.json"))
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func size(_ value: Any?) throws -> CGSize {
        let pair = try XCTUnwrap(value as? [Double])
        return CGSize(width: pair[0], height: pair[1])
    }

    private func view(_ value: Any?) throws -> ViewerView {
        let v = try XCTUnwrap(value as? [String: Double])
        return ViewerView(scale: Float(try XCTUnwrap(v["scale"])), x: Float(try XCTUnwrap(v["x"])), y: Float(try XCTUnwrap(v["y"])))
    }

    /// Two views the same to a thousandth, as the cases agree.
    private func assertSame(_ a: ViewerView, _ b: ViewerView, _ why: String, line: UInt = #line) {
        XCTAssertEqual(a.scale, b.scale, accuracy: 0.001, why, line: line)
        XCTAssertEqual(a.x, b.x, accuracy: 0.001, why, line: line)
        XCTAssertEqual(a.y, b.y, accuracy: 0.001, why, line: line)
    }

    func testAPictureOpensFittedAndNeverGrownAndZoomsToFourTimesItsOwnSize() throws {
        // The largest display picture on a phone: fitted across the room,
        // its own size six and a half times that, and the limit from its
        // own size, not from fitted.
        let phone = ViewerZoom(pixels: CGSize(width: 2560, height: 1920), room: CGSize(width: 390, height: 700))
        XCTAssertEqual(phone.fitted, CGSize(width: 390, height: 292.5))
        XCTAssertEqual(phone.frame.own, 6.5641, accuracy: 0.001)
        XCTAssertEqual(phone.furthestScale, 26.2564, accuracy: 0.001)
        XCTAssertEqual(phone.frame.detailScale, 6.5641, accuracy: 0.001)
        // One smaller than the room is drawn at its own size, not stretched.
        let small = ViewerZoom(pixels: CGSize(width: 400, height: 300), room: CGSize(width: 1000, height: 800))
        XCTAssertEqual(small.fitted, CGSize(width: 400, height: 300))
        XCTAssertEqual(small.furthestScale, 4)
        XCTAssertEqual(small.frame.detailScale, 2)

        // Every agreed frame a phone can have: it keeps no margin.
        let frames = try XCTUnwrap(try viewerCases()["frames"] as? [[String: Any]]).filter { ($0["margin"] as? Double) == 0 }
        XCTAssertGreaterThanOrEqual(frames.count, 4)
        for c in frames {
            let why = c["why"] as? String ?? ""
            let zoom = ViewerZoom(pixels: try size(c["own"]), room: try size(c["room"]))
            XCTAssertEqual(zoom.fitted.width, try XCTUnwrap(c["width"] as? Double), accuracy: 0.001, why)
            XCTAssertEqual(zoom.fitted.height, try XCTUnwrap(c["height"] as? Double), accuracy: 0.001, why)
            XCTAssertEqual(Double(zoom.frame.own), try XCTUnwrap(c["own_scale"] as? Double), accuracy: 0.001, why)
            XCTAssertEqual(zoom.furthestScale, try XCTUnwrap(c["max_scale"] as? Double), accuracy: 0.001, why)
            // Two taps in the middle of a fitted picture go to the detail scale.
            let tapped = zoom.doubleTapped(at: CGPoint(x: zoom.room.width / 2, y: zoom.room.height / 2), scale: ViewerZoom.fittedScale, offset: .zero)
            XCTAssertEqual(Double(tapped.scale), try XCTUnwrap(c["detail_scale"] as? Double), accuracy: 0.001, why)
        }
    }

    func testTheViewersScrollViewCarriesOutTheAgreedMoves() throws {
        let moves = try XCTUnwrap(try viewerCases()["moves"] as? [[String: Any]]).filter { (($0["picture"] as? [String: Any])?["margin"] as? Double) == 0 }
        var carried: [String: Int] = [:]
        for c in moves {
            let why = c["why"] as? String ?? ""
            let picture = try XCTUnwrap(c["picture"] as? [String: Any])
            let zoom = ViewerZoom(pixels: try size(picture["own"]), room: try size(picture["room"]))
            let from = try view(c["view"]), gives = try view(c["gives"])
            let move = try XCTUnwrap(c["move"] as? [String: Any])
            let kind = try XCTUnwrap(move["kind"] as? String)

            // Whatever the move, what it gives is something the scroll view
            // shows: a zoom scale inside its limits, and an offset inside
            // its content that reads back as the same view.
            XCTAssertGreaterThanOrEqual(CGFloat(gives.scale), ViewerZoom.fittedScale, why)
            XCTAssertLessThanOrEqual(CGFloat(gives.scale), zoom.furthestScale + 0.001, why)
            let rests = zoom.offset(showing: gives)
            let content = zoom.size(at: CGFloat(gives.scale))
            let furthest = CGPoint(x: max(0, content.width - zoom.room.width), y: max(0, content.height - zoom.room.height))
            XCTAssertTrue((-0.001...furthest.x + 0.001).contains(rests.x) && (-0.001...furthest.y + 0.001).contains(rests.y), "\(rests) is past the content: \(why)")
            assertSame(zoom.view(scale: CGFloat(gives.scale), offset: rests), gives, why)

            switch kind {
            case "pressed":
                // Two taps, at a point given from the room's centre.
                let at = try XCTUnwrap(move["at"] as? [Double])
                let point = CGPoint(x: zoom.room.width / 2 + at[0], y: zoom.room.height / 2 + at[1])
                assertSame(zoom.doubleTapped(at: point, scale: CGFloat(from.scale), offset: zoom.offset(showing: from)), gives, why)
            case "pan_by":
                // A drag is the scroll view's own: its offset moves against
                // the finger and stops at its content's edges.
                let by = try XCTUnwrap(move["by"] as? [Double])
                let start = zoom.offset(showing: from)
                let size = zoom.size(at: CGFloat(from.scale))
                let dragged = CGPoint(
                    x: min(max(start.x - by[0], 0), max(0, size.width - zoom.room.width)),
                    y: min(max(start.y - by[1], 0), max(0, size.height - zoom.room.height))
                )
                assertSame(zoom.view(scale: CGFloat(from.scale), offset: dragged), gives, why)
            case "held":
                // The room changed under a view: kept inside the new one.
                assertSame(zoom.held(from), gives, why)
            default:
                // A pinch is the scroll view's own, and the buttons and keys
                // are the web's and the desktop's.
                break
            }
            carried[kind, default: 0] += 1
        }
        for kind in ["pressed", "pan_by", "held"] { XCTAssertGreaterThan(carried[kind] ?? 0, 0, "no \(kind) among the cases") }
    }

    func testAPictureStaysInTheMiddleAlongASideThatFitsTheRoom() {
        // 390 by 292.5 fitted in a room 700 tall: the room's middle both ways.
        let zoom = ViewerZoom(pixels: CGSize(width: 2560, height: 1920), room: CGSize(width: 390, height: 700))
        XCTAssertEqual(zoom.centre(at: 1), CGPoint(x: 195, y: 350))
        // Twice that is 780 by 585: its own middle across, where it is
        // longer than the room, and still the room's middle down.
        XCTAssertEqual(zoom.centre(at: 2), CGPoint(x: 390, y: 350))
        XCTAssertEqual(zoom.offset(showing: ViewerView(scale: 2, x: 0, y: 0)), CGPoint(x: 195, y: 0))
        // Four times is 1,560 by 1,170: longer both ways.
        XCTAssertEqual(zoom.centre(at: 4), CGPoint(x: 780, y: 585))
        // A view dragged past the picture's edge is shown at the edge.
        XCTAssertEqual(zoom.offset(showing: ViewerView(scale: 2, x: 900, y: 40)), .zero)
        XCTAssertEqual(zoom.offset(showing: ViewerView(scale: 2, x: -900, y: 40)), CGPoint(x: 390, y: 0))
        // Pulled in past fitted, as a pinch's bounce is: smaller, in the middle.
        XCTAssertEqual(zoom.centre(at: 0.5), CGPoint(x: 195, y: 350))
    }

    func testAPicturesOwnSizeIsItsAttachmentsElseAsDecoded() {
        let decoded = CGSize(width: 1080, height: 810)
        XCTAssertEqual(ViewerZoom.pixels(width: 2560, height: 1920, decoded: decoded), CGSize(width: 2560, height: 1920), "the thumbnail is drawn at the display picture's size")
        XCTAssertEqual(ViewerZoom.pixels(width: 0, height: 0, decoded: decoded), decoded)
        XCTAssertEqual(ViewerZoom.pixels(width: 2560, height: 0, decoded: decoded), decoded)
    }

    // MARK: Words

    func testWhatIsHeardOfEachKind() {
        XCTAssertEqual(MediaWords.label(kind: .image, name: "IMG_1.jpg", alt: "A dog on a beach", sizeBytes: 1000, durationMs: 0), "Photo, A dog on a beach")
        XCTAssertEqual(MediaWords.label(kind: .image, name: "IMG_1.jpg", alt: "", sizeBytes: 1000, durationMs: 0), "Photo, IMG_1.jpg")
        XCTAssertEqual(MediaWords.label(kind: .gif, name: "party.gif", alt: "", sizeBytes: 1000, durationMs: 0), "GIF, party.gif")
        XCTAssertEqual(MediaWords.label(kind: .video, name: "clip.mov", alt: "", sizeBytes: 1000, durationMs: 7400), "Video, clip.mov, 0:07")
        XCTAssertEqual(MediaWords.label(kind: .file, name: "notes.pdf", alt: "", sizeBytes: 1_200_000, durationMs: 0), "File, notes.pdf, \(MediaWords.size(1_200_000))")
        XCTAssertEqual(MediaWords.label(kind: .audio, name: "memo.m4a", alt: "", sizeBytes: 5000, durationMs: 65000), "Audio, memo.m4a, 1:05, \(MediaWords.size(5000))")
        // Drawn as a file row, a picture is a file.
        XCTAssertEqual(MediaWords.label(kind: .image, name: "IMG_1.jpg", alt: "dog", sizeBytes: 1000, durationMs: 0, asFile: true), "File, IMG_1.jpg, \(MediaWords.size(1000))")
        XCTAssertEqual(MediaWords.cardLabel(card("https://example.com/x", title: "Example")), "Link, example.com, Example")
        XCTAssertEqual(MediaWords.refusal(code: "PERMISSION_DENIED", message: "x"), "You can’t attach files here.")
        XCTAssertEqual(MediaWords.refusal(code: nil, message: "This picture can’t be made small enough."), "This picture can’t be made small enough.")
    }

    func testAServerOutOfSpaceSaysSoOnTheFile() {
        XCTAssertEqual(MediaWords.refusal(code: "RESOURCE_EXHAUSTED", details: ["state": "storage", "quota": "5000000000"], message: "server storage is full"), "This server is out of space for files.")
        // Another quota keeps its own words.
        XCTAssertEqual(MediaWords.refusal(code: "RESOURCE_EXHAUSTED", details: ["state": "history"], message: "over quota"), "over quota")
        XCTAssertEqual(MediaWords.refusal(code: "RESOURCE_EXHAUSTED", message: "over quota"), "over quota")
    }

    func testAnAttachmentTheServerLetGoIsAFileNoLongerAvailable() {
        XCTAssertEqual(MediaWords.expired, "No longer available")
        // Whatever it was, it is heard as a file, with no size.
        XCTAssertEqual(MediaWords.label(attachment("a", expired: true)), "File, a.jpg, no longer available")
        XCTAssertEqual(MediaWords.label(attachment("v", kind: "video", expired: true)), "File, v.jpg, no longer available")
        XCTAssertEqual(MediaWords.label(attachment("a")), "Photo, a.jpg")
    }

    func testLengthsSizesAndSymbols() {
        XCTAssertEqual(MediaWords.duration(ms: 0), "0:00")
        XCTAssertEqual(MediaWords.duration(ms: 750_000), "12:30")
        XCTAssertEqual(MediaWords.duration(ms: 3_723_000), "1:02:03")
        XCTAssertEqual(MediaWords.detail(kind: .video, sizeBytes: 2048, durationMs: 7000), "0:07 · \(MediaWords.size(2048))")
        XCTAssertEqual(MediaWords.detail(kind: .file, sizeBytes: 2048, durationMs: 7000), MediaWords.size(2048))
        XCTAssertEqual(MediaWords.symbol(kind: .file, mediaType: "application/pdf", name: "a"), "doc.richtext")
        XCTAssertEqual(MediaWords.symbol(kind: .file, mediaType: "application/octet-stream", name: "a.zip"), "doc.zipper")
        XCTAssertEqual(MediaWords.symbol(kind: .file, mediaType: "text/plain", name: "a"), "doc.text")
        XCTAssertEqual(MediaWords.symbol(kind: .file, mediaType: "", name: "a.bin"), "doc")
        XCTAssertEqual(MediaWords.symbol(kind: .video, mediaType: "video/mp4", name: "a.mp4"), "film")
        XCTAssertEqual(AttachmentKind("gif"), .gif)
        XCTAssertEqual(AttachmentKind("something new"), .file)
        XCTAssertTrue(AttachmentKind.video.visual)
        XCTAssertFalse(AttachmentKind.audio.visual)
    }

    // MARK: Editing

    func testAnEditThatTakesMediaOffSavesEvenWithTheWordsAsTheyWere() {
        XCTAssertEqual(MessageEdits.outcome(original: "hi", draft: "hi"), .unchanged)
        XCTAssertEqual(MessageEdits.outcome(original: "hi", draft: "hi", mediaChanged: true, mediaLeft: true), .save("hi"))
        // Emptied beside a file still carried: saved, not deleted.
        XCTAssertEqual(MessageEdits.outcome(original: "hi", draft: " ", mediaLeft: true), .save(""))
        XCTAssertEqual(MessageEdits.outcome(original: "hi", draft: " ", mediaChanged: true, mediaLeft: false), .delete)
        XCTAssertEqual(MessageEdits.outcome(original: "", draft: "", mediaLeft: true), .unchanged)
    }

    func testWhatAnEditDropsAndKeeps() {
        var edit = MediaEdit()
        let all = [attachment("1"), attachment("2")]
        let cards = [card("https://a.dev"), card("https://b.dev")]
        XCTAssertFalse(edit.changed)
        XCTAssertNil(edit.cardURLs(cards), "no card dropped keeps those still linked")
        edit.removedAttachments.insert("2")
        edit.removedCards.insert("https://a.dev")
        XCTAssertTrue(edit.changed)
        XCTAssertEqual(edit.removing(all), ["2"])
        XCTAssertEqual(edit.cardURLs(cards), ["https://b.dev"])
        XCTAssertTrue(edit.leaves(all))
        edit.removedAttachments.insert("1")
        XCTAssertFalse(edit.leaves(all))
    }

    // MARK: What the platform decodes

    /// A picture encoded by ImageIO, with an orientation when one is given.
    private func encoded(_ type: UTType, width: Int, height: Int, alpha: UInt8 = 255, orientation: Int? = nil) throws -> Data {
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let context = try XCTUnwrap(CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        // Red on the left half, blue on the right, at the alpha given.
        let a = CGFloat(alpha) / 255
        context.setFillColor(CGColor(srgbRed: 1, green: 0, blue: 0, alpha: a))
        context.fill(CGRect(x: 0, y: 0, width: width / 2, height: height))
        context.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 1, alpha: a))
        context.fill(CGRect(x: width / 2, y: 0, width: width - width / 2, height: height))
        let image = try XCTUnwrap(context.makeImage())
        let out = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(out, type.identifier as CFString, 1, nil))
        var props: [CFString: Any] = [:]
        if let orientation { props[kCGImagePropertyOrientation] = orientation }
        CGImageDestinationAddImage(destination, image, props as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        return out as Data
    }

    private func pixel(_ p: Pixels, x: Int, y: Int) -> [UInt8] {
        let n = (y * Int(p.width) + x) * 4
        return Array(p.rgba[n..<(n + 4)])
    }

    func testAStillIsHandedOverUprightAsRGBA() throws {
        // Orientation 6: stored 40×20, shown turned a quarter clockwise, 20×40.
        let jpeg = try encoded(.jpeg, width: 40, height: 20, orientation: 6)
        XCTAssertEqual(MediaInput.nature(name: "photo.jpg", data: jpeg), .still)
        let input = MediaInput.picture(data: jpeg, name: "photo.jpg")
        XCTAssertEqual(input.bytes, jpeg, "the bytes go as chosen")
        let pixels = try XCTUnwrap(input.pixels)
        XCTAssertEqual([pixels.width, pixels.height], [20, 40])
        XCTAssertEqual(pixels.rgba.count, 20 * 40 * 4)
        // The left half, red, is now on top.
        let top = pixel(pixels, x: 10, y: 2)
        let bottom = pixel(pixels, x: 10, y: 37)
        XCTAssertGreaterThan(top[0], 200)
        XCTAssertLessThan(top[2], 60)
        XCTAssertGreaterThan(bottom[2], 200)
        XCTAssertEqual(top[3], 255)
    }

    func testAHEICIsDecodedHereSinceTheCoreCannot() throws {
        guard (CGImageDestinationCopyTypeIdentifiers() as? [String] ?? []).contains(UTType.heic.identifier) else {
            throw XCTSkip("this machine writes no HEIC")
        }
        let heic = try encoded(.heic, width: 64, height: 48)
        let input = MediaInput.picture(data: heic, name: "IMG_0001.HEIC")
        let pixels = try XCTUnwrap(input.pixels)
        XCTAssertEqual([pixels.width, pixels.height], [64, 48])
    }

    func testTransparencyIsHandedOverUnpremultiplied() throws {
        let png = try encoded(.png, width: 8, height: 8, alpha: 128)
        let pixels = try XCTUnwrap(MediaInput.picture(data: png, name: "a.png").pixels)
        let p = pixel(pixels, x: 1, y: 1)
        XCTAssertEqual(Int(p[3]), 128, accuracy: 1)
        // Premultiplied, red would read about 128; straight, about 255.
        XCTAssertGreaterThan(p[0], 240)
    }

    func testAGIFIsTheCoresToReadAndTheBoundsComeBeforeDecoding() throws {
        let gif = try encoded(.gif, width: 8, height: 8)
        XCTAssertEqual(MediaInput.nature(name: "party.gif", data: gif), .gif)
        XCTAssertNil(MediaInput.picture(data: gif, name: "party.gif").pixels, "a GIF's frames are kept")
        // Wider than 8,192 px: never decoded here, and the core decides.
        let wide = try encoded(.png, width: 9000, height: 1)
        XCTAssertEqual(MediaInput.size(of: wide)?.width, 9000)
        XCTAssertNil(MediaInput.upright(wide, longest: MediaInput.longestEdge))
        XCTAssertFalse(MediaInput.withinBounds(width: 7000, height: 7000), "over 40 million pixels")
        XCTAssertTrue(MediaInput.withinBounds(width: 8192, height: 4000))
        // Not a picture at all.
        XCTAssertNil(MediaInput.picture(data: Data("hello".utf8), name: "notes.txt").pixels)
        XCTAssertEqual(MediaInput.nature(name: "notes.txt", data: Data("hello".utf8)), .other)
        XCTAssertEqual(MediaInput.nature(name: "clip.MOV"), .video)
        XCTAssertEqual(MediaInput.nature(name: "memo.m4a"), .audio)
    }

    func testAPictureSomeoneSentIsDecodedOnlyWithinTheBounds() throws {
        let png = try encoded(.png, width: 30, height: 10)
        XCTAssertEqual(MediaInput.drawable(png)?.width, 30)
        XCTAssertNil(MediaInput.drawable(Data("not a picture".utf8)))
        let wide = try encoded(.png, width: 9000, height: 1)
        XCTAssertNil(MediaInput.drawable(wide), "over the bounds: a file row")
    }

    /// A thumbnail is at most 1,080 px (specs/media.md 3.1) and a message
    /// draws it at the edge it is decoded at here: all of it, where at 800
    /// it was made smaller and then stretched.
    func testAThumbnailIsDecodedWholeForAMessage() throws {
        let thumbnail = try encoded(.jpeg, width: 1080, height: 810)
        let whole = try XCTUnwrap(MediaInput.drawable(thumbnail, longest: MediaInput.thumbnailEdge))
        XCTAssertEqual([whole.width, whole.height], [1080, 810])
        XCTAssertEqual(MediaInput.drawable(thumbnail, longest: 800)?.width, 800)
        // One smaller than that is never enlarged.
        let small = try encoded(.jpeg, width: 400, height: 300)
        XCTAssertEqual(MediaInput.drawable(small, longest: MediaInput.thumbnailEdge)?.width, 400)
    }

    func testPixelsBecomeAnImageToDraw() throws {
        let pixels = Pixels(rgba: Data(repeating: 200, count: 3 * 2 * 4), width: 3, height: 2)
        let image = try XCTUnwrap(MediaInput.image(pixels))
        XCTAssertEqual([image.width, image.height], [3, 2])
        XCTAssertNil(MediaInput.image(Pixels(rgba: Data(count: 4), width: 3, height: 2)), "too few bytes")
    }

    func testAFileTooLargeIsRefusedBeforeItIsRead() {
        let limits = UploadLimits(imageMaxBytes: 2 << 20, fileMaxBytes: 10 << 20, thumbnailMaxBytes: 256 << 10)
        XCTAssertNil(MediaInput.refusal(size: 50 << 20, nature: .still, limits: limits), "the core makes a picture smaller")
        XCTAssertNil(MediaInput.refusal(size: 1 << 20, nature: .video, limits: limits))
        XCTAssertEqual(MediaInput.refusal(size: 50 << 20, nature: .video, limits: limits), MediaWords.tooLarge(50 << 20, max: 10 << 20))
    }

    /// A short H.264 movie of solid frames, its track turned a quarter.
    private func movie(width: Int, height: Int, frames: Int) async throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).mov")
        let writer = try AVAssetWriter(outputURL: url, fileType: .mov)
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: width, AVVideoHeightKey: height])
        input.transform = CGAffineTransform(rotationAngle: .pi / 2)
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA, kCVPixelBufferWidthKey as String: width, kCVPixelBufferHeightKey as String: height])
        writer.add(input)
        XCTAssertTrue(writer.startWriting())
        writer.startSession(atSourceTime: .zero)
        for n in 0..<frames {
            while !input.isReadyForMoreMediaData { try await Task.sleep(for: .milliseconds(5)) }
            var buffer: CVPixelBuffer?
            CVPixelBufferPoolCreatePixelBuffer(nil, try XCTUnwrap(adaptor.pixelBufferPool), &buffer)
            let frame = try XCTUnwrap(buffer)
            CVPixelBufferLockBaseAddress(frame, [])
            memset(CVPixelBufferGetBaseAddress(frame), 0x80, CVPixelBufferGetDataSize(frame))
            CVPixelBufferUnlockBaseAddress(frame, [])
            adaptor.append(frame, withPresentationTime: CMTime(value: CMTimeValue(n), timescale: 10))
        }
        input.markAsFinished()
        await writer.finishWriting()
        XCTAssertEqual(writer.status, .completed)
        return url
    }

    // MARK: How an attachment is drawn, and its file

    func testARecordingsRowPlaysItWithImagesAsLinksToo() {
        XCTAssertEqual(AttachmentDrawn.of(attachment("a", kind: "audio"), asLinks: false), .recording)
        XCTAssertEqual(AttachmentDrawn.of(attachment("a", kind: "audio"), asLinks: true), .recording, "a recording's row plays it on a press, never the share sheet")
        XCTAssertEqual(AttachmentDrawn.of(attachment("v", kind: "video"), asLinks: false), .picture, "by its poster")
        XCTAssertEqual(AttachmentDrawn.of(attachment("v", kind: "video"), asLinks: true), .videoRow, "its row plays it under itself")
        for kind in ["image", "gif"] {
            XCTAssertEqual(AttachmentDrawn.of(attachment("p", kind: kind), asLinks: false), .picture)
            XCTAssertEqual(AttachmentDrawn.of(attachment("p", kind: kind), asLinks: true), .fileRow, "a row that opens the viewer")
        }
        XCTAssertEqual(AttachmentDrawn.of(attachment("f", kind: "file"), asLinks: false), .fileRow)
        XCTAssertEqual(AttachmentDrawn.of(attachment("f", kind: "file"), asLinks: true), .fileRow)
        XCTAssertEqual(AttachmentDrawn.of(attachment("?", kind: "hologram"), asLinks: false), .fileRow, "a kind not known here is a file")
        for kind in ["image", "gif", "video", "audio", "file"] {
            for asLinks in [false, true] {
                XCTAssertEqual(AttachmentDrawn.of(attachment("x", kind: kind, expired: true), asLinks: asLinks), .expired, "one its server let go opens and plays nothing")
            }
        }
    }

    func testEveryUseOfAnAttachmentHasAFileOfItsOwn() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-media-tests-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let fm = FileManager.default
        // A recording playing from its file, then shared, and the sheet dismissed.
        let playing = try MediaFiles.write(Data("tone".utf8), named: "tone.m4a", under: root)
        let shared = try MediaFiles.write(Data("tone".utf8), named: "tone.m4a", under: root)
        XCTAssertNotEqual(playing, shared)
        XCTAssertEqual(playing.lastPathComponent, "tone.m4a", "under its own name, which the share sheet shows")
        XCTAssertEqual(shared.lastPathComponent, "tone.m4a")
        MediaFiles.remove(shared)
        XCTAssertFalse(fm.fileExists(atPath: shared.path))
        XCTAssertFalse(fm.fileExists(atPath: shared.deletingLastPathComponent().path), "with its folder")
        XCTAssertEqual(try Data(contentsOf: playing), Data("tone".utf8), "the player's file is still there")
        // And the other way: the conversation left while a share sheet holds a file.
        let again = try MediaFiles.write(Data("tone".utf8), named: "tone.m4a", under: root)
        MediaFiles.remove(playing)
        XCTAssertTrue(fm.fileExists(atPath: again.path))

        XCTAssertEqual(try MediaFiles.write(Data(), named: "a/b.txt", under: root).lastPathComponent, "a-b.txt")
        for odd in ["", ".", ".."] {
            let url = try MediaFiles.write(Data("x".utf8), named: odd, under: root)
            XCTAssertEqual(url.lastPathComponent, "attachment", "a name that is no file's")
            XCTAssertEqual(url.deletingLastPathComponent().deletingLastPathComponent().lastPathComponent, "lumia-media")
        }

        // Only what `write` made is removed.
        let stray = root.appendingPathComponent("kept/file.txt")
        try fm.createDirectory(at: stray.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("kept".utf8).write(to: stray)
        MediaFiles.remove(stray)
        XCTAssertTrue(fm.fileExists(atPath: stray.path))
        MediaFiles.remove(nil)

        // At launch, whatever a run left behind goes.
        MediaFiles.clear(under: root)
        XCTAssertFalse(fm.fileExists(atPath: again.path))
        XCTAssertFalse(fm.fileExists(atPath: root.appendingPathComponent("lumia-media").path))
        XCTAssertTrue(fm.fileExists(atPath: stray.path))
    }

    func testAVideosPosterLengthAndTurnedSizeComeFromAVFoundation() async throws {
        let url = try await movie(width: 64, height: 32, frames: 20)
        defer { try? FileManager.default.removeItem(at: url) }
        XCTAssertEqual(MediaInput.nature(name: url.lastPathComponent), .video)
        let (input, poster) = try await MediaInput.video(url: url, name: "clip.mov")
        XCTAssertEqual(input.bytes, try Data(contentsOf: url))
        // Turned a quarter: it plays 32 wide and 64 high, and so does its poster.
        XCTAssertEqual([input.videoWidth, input.videoHeight], [32, 64])
        let frame = try XCTUnwrap(input.poster)
        XCTAssertEqual([frame.width, frame.height], [32, 64])
        XCTAssertNotNil(poster)
        XCTAssertEqual(Double(try XCTUnwrap(input.durationMs)), 2000, accuracy: 150)
    }
}
