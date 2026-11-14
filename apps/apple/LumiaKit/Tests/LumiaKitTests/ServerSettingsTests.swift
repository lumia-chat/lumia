// The Server settings screen's rules (architecture 6.7): the name a server
// may have, the retention choices and which of them delete messages, the
// words for what the instance refuses, and where a picture is.
import Foundation
@testable import LumiaKit
import XCTest

final class ServerSettingsTests: XCTestCase {
    func testANameIsOneToAHundredCharactersOnceTrimmed() {
        XCTAssertTrue(ServerName.isValid("Our place"))
        XCTAssertTrue(ServerName.isValid("  x  "))
        XCTAssertTrue(ServerName.isValid(String(repeating: "a", count: 100)))
        XCTAssertFalse(ServerName.isValid(String(repeating: "a", count: 101)))
        XCTAssertFalse(ServerName.isValid(""))
        XCTAssertFalse(ServerName.isValid(" \n\t "))
        // Characters as a person counts them, as the instance does.
        XCTAssertTrue(ServerName.isValid(String(repeating: "👩‍👩‍👧", count: 100)))
        XCTAssertEqual(ServerName.trimmed("  Our place \n"), "Our place")
    }

    func testSaveIsOfferedForSomethingThatIsNotTheNameAlready() {
        XCTAssertTrue(ServerName.canSave("New name", current: "Old name"))
        XCTAssertFalse(ServerName.canSave("Old name", current: "Old name"))
        XCTAssertFalse(ServerName.canSave("  Old name ", current: "Old name"))
        XCTAssertFalse(ServerName.canSave("   ", current: "Old name"))
        XCTAssertFalse(ServerName.canSave("", current: "Old name"))
    }

    func testTheChoicesAreForeverAYearNinetyAndThirtyDays() {
        XCTAssertEqual(Retention.choices(current: 0), [0, 365, 90, 30])
        XCTAssertEqual(Retention.choices(current: 90), [0, 365, 90, 30])
        XCTAssertEqual(Retention.choices(current: 0).map(Retention.label), ["Forever", "1 year", "90 days", "30 days"])
    }

    func testAValueSetElsewhereIsAnExtraChoice() {
        XCTAssertEqual(Retention.choices(current: 45), [0, 365, 90, 30, 45])
        XCTAssertEqual(Retention.label(45), "45 days")
        XCTAssertEqual(Retention.label(1), "1 day")
    }

    func testShorterIsLessTimeAndForeverIsLongest() {
        XCTAssertTrue(Retention.isShorter(30, than: 0))
        XCTAssertTrue(Retention.isShorter(365, than: 0))
        XCTAssertTrue(Retention.isShorter(30, than: 90))
        XCTAssertFalse(Retention.isShorter(90, than: 30))
        XCTAssertFalse(Retention.isShorter(0, than: 30))
        XCTAssertFalse(Retention.isShorter(0, than: 0))
        XCTAssertFalse(Retention.isShorter(90, than: 90))
    }

    func testTheWarningNamesWhatGoes() {
        XCTAssertEqual(Retention.warning(chosen: 365, current: 0), "Messages older than 1 year will be deleted for everyone. This cannot be undone.")
        XCTAssertEqual(Retention.warning(chosen: 30, current: 90), "Messages older than 30 days will be deleted for everyone. This cannot be undone.")
        XCTAssertNil(Retention.warning(chosen: 0, current: 30))
        XCTAssertNil(Retention.warning(chosen: 90, current: 30))
        XCTAssertNil(Retention.warning(chosen: 90, current: 90))
    }

    func testShowDeletedChangedNamesTheServersToReadAgain() {
        let was = [(serverID: "a", show: false), (serverID: "b", show: true), (serverID: "c", show: false)]
        XCTAssertEqual(ShowDeleted.changed(from: was, to: was), [])
        // Turned on in a, off in b; c as it was; d new, with nothing read under it yet.
        let now = [(serverID: "a", show: true), (serverID: "b", show: false), (serverID: "c", show: false), (serverID: "d", show: true)]
        XCTAssertEqual(ShowDeleted.changed(from: was, to: now), ["a", "b"])
        // The first snapshot changes nothing that was read.
        XCTAssertEqual(ShowDeleted.changed(from: [], to: now), [])
    }

    func testSettingsRefusals() {
        XCTAssertEqual(ServerSettingsRefusal.settings(code: "PERMISSION_DENIED", details: [:], message: "no"), "You cannot change this server's settings.")
        XCTAssertEqual(ServerSettingsRefusal.settings(code: "INVALID_ARGUMENT", details: ["field": "name"], message: "bad"), "A server name is 1 to 100 characters.")
        XCTAssertEqual(ServerSettingsRefusal.settings(code: "INVALID_ARGUMENT", details: ["field": "retention_days"], message: "bad"), "bad")
        XCTAssertEqual(ServerSettingsRefusal.settings(code: nil, details: [:], message: "offline"), "offline")
    }

    func testPictureRefusalsAreTheProfilePhotos() {
        let say = { (code: String?, reason: String?) in
            ServerSettingsRefusal.picture(code: code, details: reason.map { ["reason": $0] } ?? [:], message: "other")
        }
        XCTAssertEqual(say("INVALID_ARGUMENT", "too_large"), "That picture is too large. Choose one under 8 MB.")
        XCTAssertEqual(say("INVALID_ARGUMENT", "unsupported"), "That file is not a picture Lumia can read. Choose a PNG, JPEG, WebP or GIF.")
        XCTAssertEqual(say("INVALID_ARGUMENT", nil), "That picture could not be read.")
        XCTAssertEqual(say("FAILED_PRECONDITION", "blobs"), "This instance does not keep pictures.")
        XCTAssertEqual(say("RATE_LIMITED", nil), "The instance is busy. Try again in a moment.")
        XCTAssertEqual(say("PERMISSION_DENIED", nil), "You cannot change this server's settings.")
        XCTAssertEqual(say(nil, nil), "other")
    }

    func testOnlyTheInstancesOwnPicturesAreFetched() {
        let base = URL(string: "https://chat.example.com/")!
        let name = String(repeating: "0123456789abcdef", count: 4)
        XCTAssertEqual(pictureURL("/api/v1/avatars/\(name)", base: base)?.absoluteString, "https://chat.example.com/api/v1/avatars/\(name)")
        XCTAssertEqual(pictureURL("/api/v1/avatars/\(name)", base: URL(string: "http://localhost:4031")!)?.absoluteString, "http://localhost:4031/api/v1/avatars/\(name)")
        XCTAssertNil(pictureURL("", base: base))
        XCTAssertNil(pictureURL("https://elsewhere.example/a.png", base: base))
        XCTAssertNil(pictureURL("/api/v1/avatars/\(name.uppercased())", base: base))
        XCTAssertNil(pictureURL("/api/v1/avatars/\(name)x", base: base))
        XCTAssertNil(pictureURL("/api/v1/avatars/../\(name)", base: base))
    }
}
