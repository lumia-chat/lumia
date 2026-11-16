// The Server settings screen's rules (architecture 6.7): the name a server
// may have, the retention choices and which of them delete messages, the
// words for what the instance refuses, a server's files, and where a picture is.
import Foundation
@testable import LumiaKit
import LumiaFFI
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

    // MARK: A server's files (specs/media.md 4.5)

    func testTheUsageLineSaysTheLimitWhereThereIsOne() {
        XCTAssertEqual(ServerFiles.usage(used: 1_200_000_000, max: 5_000_000_000), "\(ServerFiles.size(1_200_000_000)) of \(ServerFiles.size(5_000_000_000)) used")
        XCTAssertEqual(ServerFiles.usage(used: 1_200_000_000, max: 0), "\(ServerFiles.size(1_200_000_000)) used", "no limit, none said")
        XCTAssertEqual(ServerFiles.usage(StorageSummary(usedBytes: 2048, maxBytes: 0)), "\(ServerFiles.size(2048)) used")
        // Nothing yet is a number, not the formatter's "Zero KB".
        XCTAssertFalse(ServerFiles.size(0).localizedCaseInsensitiveContains("zero"), ServerFiles.size(0))
        // As the Files app says a size, where its decimal point is a point.
        if Locale.current.decimalSeparator == "." {
            XCTAssertEqual(ServerFiles.usage(used: 1_200_000_000, max: 5_000_000_000), "1.2 GB of 5 GB used")
            XCTAssertEqual(ServerFiles.usage(used: 1_200_000_000, max: 0), "1.2 GB used")
        }
    }

    func testAttachmentsAreKeptForTheSameChoicesAsMessages() {
        XCTAssertEqual(Retention.choices(current: 0).map(Retention.label), ["Forever", "1 year", "90 days", "30 days"])
        XCTAssertEqual(ServerFiles.retentionWarning(chosen: 90, current: 0), "Attachments older than 90 days will be deleted for everyone. Their messages stay. This cannot be undone.")
        XCTAssertEqual(ServerFiles.retentionWarning(chosen: 30, current: 365), "Attachments older than 30 days will be deleted for everyone. Their messages stay. This cannot be undone.")
        XCTAssertEqual(ServerFiles.retentionWarning(chosen: 365, current: 0), "Attachments older than 1 year will be deleted for everyone. Their messages stay. This cannot be undone.")
        XCTAssertNil(ServerFiles.retentionWarning(chosen: 0, current: 30), "keeping them longer deletes nothing")
        XCTAssertNil(ServerFiles.retentionWarning(chosen: 365, current: 90))
        XCTAssertNil(ServerFiles.retentionWarning(chosen: 90, current: 90))
    }

    func testWhatAFullServerDoesRefusesFirstAndSaysBeforeItDeletes() {
        XCTAssertEqual(ServerFiles.fullChoices.map(ServerFiles.fullLabel), ["Refuse new uploads", "Delete the oldest attachments"])
        XCTAssertEqual(ServerFiles.fullDescription(false), "When an upload would not fit, it is refused until files are deleted.")
        XCTAssertEqual(ServerFiles.fullDescription(true), "When an upload would not fit, the oldest messages’ attachments are deleted for everyone to make room.")
        XCTAssertTrue(ServerFiles.fullWarns(chosen: true, current: false), "choosing to delete warns before Save")
        XCTAssertFalse(ServerFiles.fullWarns(chosen: true, current: true), "already so: Save changes nothing")
        XCTAssertFalse(ServerFiles.fullWarns(chosen: false, current: true))
        XCTAssertFalse(ServerFiles.fullWarns(chosen: false, current: false))
    }

    func testSaveIsOfferedForAFileSettingThatChanged() {
        let server = ServerSummary(serverId: "s", name: "S", iconUrl: "", retentionDays: 0, showDeleted: false, attachmentRetentionDays: 90, deleteOldestWhenFull: false, canManage: true, canManageChannels: true, canStartDms: true, canMentionEveryone: true, canManageMessages: true, channelAddsByInvitation: false, serverDmsOff: false)
        XCTAssertFalse(ServerFiles.canSave(retention: 90, deleteOldest: false, server: server))
        XCTAssertTrue(ServerFiles.canSave(retention: 30, deleteOldest: false, server: server))
        XCTAssertTrue(ServerFiles.canSave(retention: 90, deleteOldest: true, server: server))
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
