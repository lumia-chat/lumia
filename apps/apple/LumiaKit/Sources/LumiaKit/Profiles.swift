import LumiaFFI

/// Someone's name or picture as it now is (issue #15): the account as
/// others see it, from `ClientEvent.profileUpdated`. Only the account's own
/// fields change; a server's nickname and switches are that server's and stay.
public struct ProfileUpdate: Equatable, Sendable {
    public let accountID: String
    public let handle: String
    public let displayName: String
    /// The new picture's address, empty when it was cleared.
    public let avatarURL: String
    public let pronouns: String
    public let bio: String

    public init(accountID: String, handle: String, displayName: String, avatarURL: String, pronouns: String, bio: String) {
        self.accountID = accountID
        self.handle = handle
        self.displayName = displayName
        self.avatarURL = avatarURL
        self.pronouns = pronouns
        self.bio = bio
    }

    /// `member` as they now are when it is this person; anyone else as they were.
    public func applied(to member: MemberSummary) -> MemberSummary {
        guard member.accountId == accountID else { return member }
        var m = member
        m.handle = handle
        m.displayName = displayName
        m.avatarUrl = avatarURL
        m.pronouns = pronouns
        m.bio = bio
        return m
    }

    /// A member list with this person as they now are, if they are in it.
    public func applied(to members: [MemberSummary]) -> [MemberSummary] {
        members.map { applied(to: $0) }
    }

    /// The snapshot with this account as it now is when the update is this
    /// account's own: the rail's avatar, the account menu and Profile draw it.
    public func applied(to snapshot: SnapshotSummary) -> SnapshotSummary {
        guard snapshot.accountId == accountID else { return snapshot }
        var s = snapshot
        s.handle = handle
        s.displayName = displayName
        s.avatarUrl = avatarURL
        return s
    }
}
