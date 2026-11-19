// A changed name or picture (issue #15): the person written over what is
// held of them, each server's nickname and switches kept, nobody else touched.
@testable import LumiaKit
import LumiaFFI
import XCTest

final class ProfilesTests: XCTestCase {
    private func member(_ id: String, name: String, nickname: String = "", avatar: String = "", invitesOnly: Bool = false, dmsOff: Bool = false) -> MemberSummary {
        MemberSummary(accountId: id, handle: id, displayName: name, nickname: nickname, pronouns: "", bio: "", avatarUrl: avatar, channelAddsByInvitation: invitesOnly, serverDmsOff: dmsOff, roleIds: [], title: "", guest: false, owner: false, tag: nil, canManage: false)
    }

    private func snapshot(_ id: String, name: String, avatar: String = "") -> SnapshotSummary {
        SnapshotSummary(accountId: id, handle: id, displayName: name, avatarUrl: avatar, servers: [], channels: [], pendingRequests: [], channelInvitations: [], readStates: [], rail: [])
    }

    private let ada = ProfileUpdate(accountID: "ada", handle: "countess", displayName: "Ada Lovelace", avatarURL: "/api/v1/avatars/new", pronouns: "she/her", bio: "Notes on the engine")

    func testAMemberIsAsTheyNowAreWithTheServersNicknameAndSwitchesKept() {
        let before = member("ada", name: "Ada", nickname: "Countess", avatar: "/api/v1/avatars/old", invitesOnly: true, dmsOff: true)
        let after = ada.applied(to: before)
        XCTAssertEqual(after.handle, "countess")
        XCTAssertEqual(after.displayName, "Ada Lovelace")
        XCTAssertEqual(after.avatarUrl, "/api/v1/avatars/new")
        XCTAssertEqual(after.pronouns, "she/her")
        XCTAssertEqual(after.bio, "Notes on the engine")
        XCTAssertEqual(after.nickname, "Countess")
        XCTAssertTrue(after.channelAddsByInvitation)
        XCTAssertTrue(after.serverDmsOff)
    }

    func testNobodyElseChanges() {
        let grace = member("grace", name: "Grace", avatar: "/api/v1/avatars/grace")
        XCTAssertEqual(ada.applied(to: grace), grace)
        let list = [member("ada", name: "Ada"), grace]
        let after = ada.applied(to: list)
        XCTAssertEqual(after.map(\.displayName), ["Ada Lovelace", "Grace"])
        XCTAssertEqual(after[1], grace)
        let theirs = snapshot("me", name: "Me", avatar: "/api/v1/avatars/me")
        XCTAssertEqual(ada.applied(to: theirs), theirs)
    }

    func testThisAccountsOwnUpdateIsTheSnapshots() {
        let mine = ProfileUpdate(accountID: "me", handle: "me", displayName: "New me", avatarURL: "", pronouns: "", bio: "")
        let after = mine.applied(to: snapshot("me", name: "Me", avatar: "/api/v1/avatars/me"))
        XCTAssertEqual(after.displayName, "New me")
        // A cleared picture: back to the initials.
        XCTAssertEqual(after.avatarUrl, "")
    }

    @MainActor
    func testTheEventRedrawsEveryoneWhoShowsThePersonAndNoOneElse() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-kit-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let client = try LumiaClient(baseURL: URL(string: "http://127.0.0.1:1")!, dataDirectory: dir)
        let session = LumiaSession(client: client, host: "127.0.0.1:1", signedIn: true, hasIdentity: false)
        let grace = member("grace", name: "Grace", avatar: "/api/v1/avatars/grace")
        session.take(snapshot("me", name: "Me", avatar: "/api/v1/avatars/me"))
        session.serverMembers = [
            "s": [member("ada", name: "Ada", avatar: "/api/v1/avatars/old"), grace],
            "t": [member("ada", name: "Ada", nickname: "Countess", avatar: "/api/v1/avatars/old", dmsOff: true)],
            "u": [grace],
        ]
        session.people = ["ada": member("ada", name: "Ada", avatar: "/api/v1/avatars/old"), "grace": grace]
        let refused: MemberSummary? = nil
        session.profiles = ["alan": member("alan", name: "Alan"), "nobody": refused]
        let held = session.serverMembers["u"]
        // What draws Ada (a row's name and avatar, the profile card) is told to draw again.
        final class Told: @unchecked Sendable { var told = false }
        let redraw = Told()
        withObservationTracking {
            _ = session.name(of: "ada")
            _ = session.picture(of: "ada")
            _ = session.serverMembers["t"]
        } onChange: { redraw.told = true }

        await session.handle(.profileUpdated(accountId: "ada", handle: "countess", displayName: "Ada Lovelace", avatarUrl: "/api/v1/avatars/new", pronouns: "she/her", bio: ""))
        XCTAssertTrue(redraw.told)
        XCTAssertEqual(session.name(of: "ada"), "Ada Lovelace")
        XCTAssertEqual(session.avatarURL(of: "ada"), "/api/v1/avatars/new")
        XCTAssertEqual(session.pronouns(of: "ada"), "she/her")
        XCTAssertEqual(session.serverMembers["s"]?.map(\.displayName), ["Ada Lovelace", "Grace"])
        XCTAssertEqual(session.serverMembers["s"]?.first?.avatarUrl, "/api/v1/avatars/new")
        let t = session.serverMembers["t"]?.first
        XCTAssertEqual(t?.displayName, "Ada Lovelace")
        XCTAssertEqual(t?.nickname, "Countess")
        XCTAssertEqual(t?.serverDmsOff, true)
        // Nothing else.
        XCTAssertEqual(session.people["grace"], grace)
        XCTAssertEqual(session.serverMembers["s"]?[1], grace)
        XCTAssertEqual(session.serverMembers["u"], held)
        XCTAssertEqual(session.selfName, "Me")
        XCTAssertEqual(session.avatarURL(of: "me"), "/api/v1/avatars/me")

        // Someone known only by a profile looked up.
        await session.handle(.profileUpdated(accountId: "alan", handle: "alan", displayName: "Alan Turing", avatarUrl: "/api/v1/avatars/alan", pronouns: "", bio: ""))
        XCTAssertEqual(session.name(of: "alan"), "Alan Turing")
        XCTAssertEqual(session.avatarURL(of: "alan"), "/api/v1/avatars/alan")
        XCTAssertNil(session.people["alan"])

        // This account: the rail's avatar, the account menu and Profile.
        await session.handle(.profileUpdated(accountId: "me", handle: "me", displayName: "Renamed", avatarUrl: "", pronouns: "", bio: ""))
        XCTAssertEqual(session.selfName, "Renamed")
        XCTAssertEqual(session.name(of: "me"), "Renamed")
        XCTAssertEqual(session.avatarURL(of: "me"), "")
        XCTAssertNil(session.picture(of: "me"))
        XCTAssertEqual(session.name(of: "ada"), "Ada Lovelace")
    }
}
