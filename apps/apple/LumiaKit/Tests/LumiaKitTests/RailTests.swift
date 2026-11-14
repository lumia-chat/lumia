// The rail's order and folders as the phone has them (architecture 6.9):
// a folder's count and words, the folders open on this phone, a tile's
// menu, where a drag drops, what is said of a move, and an edit drawn at
// once and put back when the instance does not take it.
import CoreGraphics
import Foundation
@testable import LumiaKit
import LumiaFFI
import XCTest

final class RailTests: XCTestCase {
    private func s(_ id: String) -> RailItem { .server(serverId: id) }
    private func f(_ id: String, _ servers: [String], name: String = "") -> RailItem {
        .folder(folderId: id, name: name, tone: "mint", serverIds: servers)
    }

    func testAPlaceIsOnTheRailOrInItsFolder() {
        let rail = [s("a"), f("F", ["b", "c"]), s("d")]
        XCTAssertEqual(RailPlace.of("a", in: rail), RailPlace(folder: nil, index: 0, count: 3))
        XCTAssertEqual(RailPlace.of("F", in: rail), RailPlace(folder: nil, index: 1, count: 3))
        XCTAssertEqual(RailPlace.of("c", in: rail), RailPlace(folder: "F", index: 1, count: 2))
        XCTAssertNil(RailPlace.of("x", in: rail))
        XCTAssertEqual(rail[1].serverIDs, ["b", "c"])
        XCTAssertEqual(rail[0].serverIDs, ["a"])
    }

    func testAClosedFolderAddsUpItsMentionsElseSaysUnread() {
        let sum = RailCounts.folder([(false, 0), (true, 2), (true, 1)])
        XCTAssertEqual(sum.mentions, 3)
        XCTAssertTrue(sum.unread)
        let quiet = RailCounts.folder([(false, 0), (true, 0)])
        XCTAssertEqual(quiet.mentions, 0)
        XCTAssertTrue(quiet.unread)
        XCTAssertFalse(RailCounts.folder([]).unread)
    }

    func testAFolderReadsAsItsNameElseItsServers() {
        XCTAssertEqual(RailWords.folderLabel(name: "  Work ", words: "Acme and Lumia"), "Work")
        XCTAssertEqual(RailWords.folderLabel(name: " ", words: "Acme and Lumia"), "Acme and Lumia")
        XCTAssertEqual(RailWords.folderSpoken("Work"), "Folder, Work")
        XCTAssertEqual(RailWords.folderValue(open: false, unread: true, mentions: 3), "collapsed, 3 mentions")
        XCTAssertEqual(RailWords.folderValue(open: true, unread: true, mentions: 0), "expanded, unread")
        XCTAssertEqual(RailWords.folderValue(open: true, unread: false, mentions: 0), "expanded")
        XCTAssertEqual(RailWords.toneName(.indigo), "Indigo")
        XCTAssertEqual(AvatarTone.folder("sky", id: "x"), .sky)
        XCTAssertEqual(AvatarTone.folder("red", id: "x"), .of("x"))
    }

    func testTheCoreWordsAFolderWithNoName() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        XCTAssertEqual(client.railFolderWords(["Northwind Studio", "Open Source Guild", "Design", "Lumia"]), "Northwind Studio, Open Source Guild and 2 more")
        // The core's edits, as the menus make them.
        let rail = [s("a"), f("F", ["b", "c"]), s("d")]
        XCTAssertEqual(client.railStep(rail, id: "d", by: -1), [s("a"), s("d"), f("F", ["b", "c"])])
        XCTAssertEqual(client.railOutOfFolder(rail, serverID: "b"), [s("a"), f("F", ["c"]), s("b"), s("d")])
        let wrapped = client.railNewFolder(rail, serverID: "a")
        guard case let .folder(_, name, _, servers) = wrapped[0] else { return XCTFail("no new folder: \(wrapped)") }
        XCTAssertEqual(name, "")
        XCTAssertEqual(servers, ["a"])
    }

    func testARefusalIsSaidInTheWebsWords() {
        let folders = LumiaError.Api(code: "INVALID_ARGUMENT", detail: "invalid argument", details: ["field": "folders"])
        XCTAssertEqual(RailWords.refusal(folders), "There can be at most 100 folders.")
        let name = LumiaError.Api(code: "INVALID_ARGUMENT", detail: "invalid argument", details: ["field": "name"])
        XCTAssertEqual(RailWords.refusal(name), "A folder's name is up to 50 characters.")
        XCTAssertEqual(RailWords.refusal(LumiaError.Other(detail: "offline")), "offline")
    }

    func testOpenFoldersAreKeptAsOneStringOfIds() {
        var open = OpenFolders(stored: "")
        XCTAssertFalse(open.isOpen("F"))
        open.toggle("F")
        open.toggle("G")
        XCTAssertEqual(open.stored, "F,G")
        XCTAssertEqual(OpenFolders(stored: open.stored), open)
        open.toggle("F")
        XCTAssertEqual(open.stored, "G")
        XCTAssertEqual(OpenFolders(stored: "F,G,,H").kept(in: [f("G", ["a"]), s("F")]).stored, "G")
    }

    func testAServersMenuOffersWhatItCanDo() {
        let rail = [s("a"), f("F", ["b", "c"]), s("d")]
        XCTAssertEqual(RailMenu.actions(for: "a", in: rail), [.moveDown, .newFolder, .moveToFolder])
        XCTAssertEqual(RailMenu.actions(for: "d", in: rail), [.moveUp, .newFolder, .moveToFolder])
        XCTAssertEqual(RailMenu.actions(for: "b", in: rail), [.moveDown, .newFolder, .removeFromFolder])
        XCTAssertEqual(RailMenu.actions(for: "c", in: rail + [f("G", ["e"])]), [.moveUp, .newFolder, .moveToFolder, .removeFromFolder])
        XCTAssertEqual(RailMenu.folders(for: "c", in: rail + [f("G", ["e"])]), ["G"])
        XCTAssertEqual(RailMenu.folders(for: "a", in: rail), ["F"])
        XCTAssertEqual(RailMenu.actions(for: "x", in: rail), [])
        XCTAssertEqual(RailAction.folderSettings.title, "Folder settings…")
    }

    func testAFoldersMenuHasItsSettingsAndDelete() {
        let rail = [s("a"), f("F", ["b", "c"]), s("d")]
        XCTAssertEqual(RailMenu.actions(for: "F", in: rail), [.moveUp, .moveDown, .folderSettings, .deleteFolder])
        XCTAssertEqual(RailMenu.actions(for: "F", in: [f("F", ["b"])]), [.folderSettings, .deleteFolder])
    }

    /// Tiles 44 high, 12 apart from y 0: a at 0, b at 56, c at 112.
    private func column(_ ids: [String]) -> [RailSlot] {
        ids.enumerated().map { RailSlot(id: $1, frame: CGRect(x: 0, y: CGFloat($0) * 56, width: 44, height: 44)) }
    }

    func testATilesQuartersAreBetweenAndItsMiddleIsOnto() {
        let slots = column(["a", "b", "c"])
        let at = { (y: CGFloat) in RailDropTarget.at(y, dragging: "c", isFolder: false, slots: slots, gap: 12) }
        XCTAssertEqual(at(2)?.drop, .between(folder: nil, before: "a"))
        XCTAssertEqual(at(2)?.line, -6)
        XCTAssertEqual(at(22)?.drop, .onto(server: "a"))
        XCTAssertEqual(at(22)?.highlight, "a")
        XCTAssertEqual(at(42)?.drop, .between(folder: nil, before: "b"))
        XCTAssertEqual(at(42)?.line, 50)
        // The gap belongs to the tile above it.
        XCTAssertEqual(at(48)?.drop, .between(folder: nil, before: "b"))
        // Over itself, and just above itself, nothing moves.
        XCTAssertNil(at(130))
        XCTAssertNil(at(96))
        // Past the end.
        XCTAssertEqual(RailDropTarget.at(400, dragging: "a", isFolder: false, slots: slots, gap: 12)?.drop, .between(folder: nil, before: nil))
    }

    func testOntoAFolderIsIntoItAndAnOpenFoldersColumnTakesServersInPlace() {
        // a; F open: glyph at 56 (32 high), b at 98, c at 154 (its column 52 to 206); d at 218.
        let slots = [
            RailSlot(id: "a", frame: CGRect(x: 0, y: 0, width: 44, height: 44)),
            RailSlot(id: "F", isFolder: true, frame: CGRect(x: 0, y: 56, width: 44, height: 32), group: CGRect(x: 0, y: 52, width: 52, height: 154)),
            RailSlot(id: "b", folder: "F", frame: CGRect(x: 0, y: 98, width: 44, height: 44)),
            RailSlot(id: "c", folder: "F", frame: CGRect(x: 0, y: 154, width: 44, height: 44)),
            RailSlot(id: "d", frame: CGRect(x: 0, y: 218, width: 44, height: 44)),
        ]
        let at = { (y: CGFloat, id: String) in RailDropTarget.at(y, dragging: id, isFolder: false, slots: slots, gap: 12)?.drop }
        XCTAssertEqual(at(72, "a"), .into(folder: "F"))
        XCTAssertEqual(at(57, "d"), .between(folder: nil, before: "F"))
        XCTAssertEqual(at(86, "d"), .between(folder: "F", before: "b"))
        XCTAssertEqual(at(141, "d"), .between(folder: "F", before: "c"))
        XCTAssertEqual(at(176, "d"), .onto(server: "c"))
        XCTAssertEqual(at(196, "d"), .between(folder: "F", before: nil))
        // Out of the folder: between the rail's own tiles.
        XCTAssertEqual(at(220, "b"), .between(folder: nil, before: "d"))
        XCTAssertEqual(at(300, "b"), .between(folder: nil, before: nil))
    }

    func testAFolderDragsWholeAndOnlyBetween() {
        let slots = [
            RailSlot(id: "a", frame: CGRect(x: 0, y: 0, width: 44, height: 44)),
            RailSlot(id: "F", isFolder: true, frame: CGRect(x: 0, y: 56, width: 44, height: 44)),
            RailSlot(id: "d", frame: CGRect(x: 0, y: 112, width: 44, height: 44)),
        ]
        let at = { (y: CGFloat) in RailDropTarget.at(y, dragging: "F", isFolder: true, slots: slots, gap: 12)?.drop }
        XCTAssertEqual(at(10), .between(folder: nil, before: "a"))
        // After a and before d are where it is.
        XCTAssertNil(at(30))
        XCTAssertNil(at(70))
        XCTAssertNil(at(120))
        XCTAssertEqual(at(150), .between(folder: nil, before: nil))
    }

    func testAMoveIsSaid() {
        XCTAssertEqual(RailWords.moved("Acme", position: 3, into: nil, newFolder: false), "Acme moved to position 3")
        XCTAssertEqual(RailWords.moved("Acme", position: 1, into: "Work", newFolder: false), "Acme moved into Work")
        XCTAssertEqual(RailWords.moved("Acme", position: 1, into: "Acme", newFolder: true), "Acme moved into a new folder")
    }

    @MainActor
    func testAnEditIsDrawnAtOnceAndPutBackWhenRefused() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let session = await LumiaSession(client: try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir))
        XCTAssertEqual(session.rail, [])
        let sending = Task { await session.arrangeRail([s("018f0000-0000-7000-8000-000000000001")]) }
        await Task.yield()
        await sending.value
        // Not signed in: refused, and the snapshot's rail (none) drawn again.
        XCTAssertEqual(session.rail, [])
        XCTAssertNotNil(session.railError)
        session.railErrorRead()
        XCTAssertNil(session.railError)
        // A rail as it is sends nothing.
        await session.arrangeRail([])
        XCTAssertNil(session.railError)
    }
}
