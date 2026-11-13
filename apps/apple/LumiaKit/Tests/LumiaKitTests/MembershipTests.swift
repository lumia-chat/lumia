// Channel membership and server DMs (architecture 6.4 and 6.8): the
// sidebar's lists, the notices, the pickers, the default channels, the
// gateway's subscriptions and the words.
@testable import LumiaKit
import LumiaFFI
import XCTest

final class MembershipTests: XCTestCase {
    private func channel(_ id: String, server: String = "s", kind: String = "TEXT", name: String = "", participants: [String] = [], isPrivate: Bool = false, isDefault: Bool = false) -> ChannelSummary {
        ChannelSummary(channelId: id, serverId: server, name: name.isEmpty ? id : name, kind: kind, lastSeq: 0, participantAccountIds: participants, host: "", private: isPrivate, isDefault: isDefault, memberCount: 0, topic: "")
    }

    private func member(_ id: String, name: String, handle: String = "", nickname: String = "", invitesOnly: Bool = false, dmsOff: Bool = false) -> MemberSummary {
        MemberSummary(accountId: id, handle: handle.isEmpty ? id : handle, displayName: name, nickname: nickname, pronouns: "", bio: "", avatarUrl: "", channelAddsByInvitation: invitesOnly, serverDmsOff: dmsOff)
    }

    private func message(_ id: String, author: String, at: Int64, notice: String = "", about: String = "") -> MessageSummary {
        MessageSummary(messageId: id, channelId: "c", seq: 0, authorAccountId: author, body: notice.isEmpty ? "hi" : "", createdAtMicros: at, deleted: false, notice: notice, noticeAccountId: about)
    }

    func testTheSidebarListsChannelsAndServerDMsApart() {
        let all = [
            channel("general"),
            channel("dm", kind: "SERVER_DM", participants: ["me", "ada"]),
            channel("elsewhere", server: "t"),
            channel("personal", server: "", kind: "PERSONAL_DM"),
            channel("leads", isPrivate: true),
        ]
        XCTAssertEqual(Membership.channels(all, serverID: "s").map(\.channelId), ["general", "leads"])
        XCTAssertEqual(Membership.serverDMs(all, serverID: "s").map(\.channelId), ["dm"])
        XCTAssertEqual(Membership.other(in: all[1], me: "me"), "ada")
        XCTAssertEqual(Membership.other(in: channel("alone", kind: "SERVER_DM", participants: ["me"]), me: "me"), "me")
    }

    func testInvitationsAreThisServersAndReadAsTheyShould() {
        let mine = ChannelInvitationSummary(channel: channel("design", name: "design"), invitedByAccountId: "ada", createdAtMicros: 0)
        let other = ChannelInvitationSummary(channel: channel("x", server: "t"), invitedByAccountId: "ada", createdAtMicros: 0)
        XCTAssertEqual(Membership.invitations([mine, other], serverID: "s").map(\.channel.channelId), ["design"])
        XCTAssertEqual(Membership.invitationLine(channel: "design", inviter: "Ada"), "#design · Ada invited you")
    }

    func testNoticesReadAsOneLine() {
        XCTAssertEqual(noticeText(kind: "joined", actor: "Grace", subject: "Grace"), "Grace joined")
        XCTAssertEqual(noticeText(kind: "added", actor: "Ada", subject: "Grace"), "Ada added Grace")
        XCTAssertEqual(noticeText(kind: "left", actor: "Grace", subject: "Grace"), "Grace left")
        XCTAssertEqual(noticeText(kind: "removed", actor: "Ada", subject: "Grace"), "Ada removed Grace")
        XCTAssertNil(noticeText(kind: "renamed", actor: "Ada", subject: "Grace"))
    }

    func testANoticeIsABlockOfItsOwn() {
        let minute: Int64 = 60_000_000
        let list = [
            message("1", author: "ada", at: 0),
            message("2", author: "ada", at: minute, notice: "added", about: "grace"),
            message("3", author: "ada", at: 2 * minute),
            message("4", author: "ada", at: 3 * minute),
        ]
        let places = MessageBlocks.places(list.map(messageStamp))
        XCTAssertEqual(places.map(\.opens), [true, true, true, false])
        XCTAssertEqual(places.map(\.closes), [true, true, false, true])
    }

    func testPickersLeaveOutWhoTheyShould() {
        let members = [member("me", name: "Me"), member("grace", name: "Grace Hopper", handle: "grace"), member("ada", name: "Ada Lovelace", handle: "countess", nickname: "Ada")]
        let name: (MemberSummary) -> String = { $0.displayName }
        XCTAssertEqual(Membership.pick(members, me: "me", query: "", name: name).map(\.accountId), ["ada", "grace"])
        XCTAssertEqual(Membership.pick(members, me: "me", excluding: ["ada"], query: "", name: name).map(\.accountId), ["grace"])
        XCTAssertEqual(Membership.pick(members, me: "me", query: "COUNT", name: name).map(\.accountId), ["ada"])
        XCTAssertEqual(Membership.pick(members, me: "me", query: " hop ", name: name).map(\.accountId), ["grace"])
    }

    func testDefaultChannelsArePublicOnesOnceAndSaveOnlyAChange() {
        let joined = [channel("general", isDefault: true), channel("leads", isPrivate: true), channel("dm", kind: "SERVER_DM"), channel("x", server: "t")]
        let browsed = [channel("design"), channel("general")]
        let choices = Membership.publicChannels(joined: joined, browsed: browsed, serverID: "s")
        XCTAssertEqual(choices.map(\.channelId), ["general", "design"])
        var defaults = DefaultChannels(choices)
        XCTAssertTrue(defaults.isChosen("general"))
        XCTAssertFalse(defaults.changed)
        defaults.toggle("design")
        XCTAssertTrue(defaults.changed)
        XCTAssertEqual(defaults.ids(in: choices), ["general", "design"])
        defaults.toggle("design")
        XCTAssertFalse(defaults.changed)
        defaults.toggle("general")
        XCTAssertEqual(defaults.ids(in: choices), [])
    }

    func testAChannelGainedLaterIsSubscribedToOnce() {
        var subscriptions = ChannelSubscriptions()
        subscriptions.started(["general"])
        let later = [channel("general"), channel("design"), channel("dm", kind: "SERVER_DM"), channel("p", server: "", kind: "PERSONAL_DM")]
        XCTAssertEqual(subscriptions.gained(later), ["design", "dm"])
        XCTAssertEqual(subscriptions.gained(later), [])
        XCTAssertFalse(subscriptions.add("design"))
        XCTAssertTrue(subscriptions.add("random"))
        subscriptions.clear()
        XCTAssertEqual(subscriptions.gained([channel("general")]), ["general"])
    }

    func testTheWords() {
        XCTAssertEqual(Membership.memberCount(1), "1 member")
        XCTAssertEqual(Membership.memberCount(0), "0 members")
        XCTAssertEqual(Membership.memberCount(12), "12 members")
        XCTAssertEqual(Membership.notIn("design"), "You’re not in #design")
        XCTAssertEqual(Membership.serverDMNote(server: "Acme Works"), "Messages here are kept by Acme Works and follow its message history settings. For an encrypted conversation, message privately.")
        XCTAssertEqual(MembershipRefusal.serverDM(code: "FAILED_PRECONDITION", name: "Grace", message: "x"), "Grace doesn’t take messages here.")
        XCTAssertEqual(MembershipRefusal.add(code: "INVALID_ARGUMENT", message: "x"), "Someone chosen cannot be added to this channel.")
        XCTAssertEqual(MembershipRefusal.remove(code: "PERMISSION_DENIED", message: "x"), "You cannot remove people from this channel.")
        XCTAssertEqual(MembershipRefusal.remove(code: "UNAVAILABLE", message: "x"), "x")
        XCTAssertEqual(ServerSettingsRefusal.settings(code: "INVALID_ARGUMENT", details: ["field": "default_channels"], message: "x"), "Default channels must be public channels of this server.")
    }
}
