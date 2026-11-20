// The session's side of a server's own emoji (specs/emoji.md): what the
// snapshot holds of each server's, an emoji drawn by its id with its
// picture fetched through the URL cache, reacting with one, the composer's
// matches, and uploading, renaming and removing them, each holding the
// snapshot the instance answers with. `EmojiChanged` comes after the
// `accountApplied` that took the snapshot again, so it only lets go of the
// pictures no server lists any more.
import Foundation
import LumiaFFI

public extension LumiaSession {
    /// A server's live emoji as held, oldest first; none for a server not held.
    func emoji(of serverID: String) -> [EmojiSummary] {
        snapshot?.servers.first { $0.serverId == serverID }?.emoji ?? []
    }

    /// One of a server's emoji as a view draws it, its picture asked for
    /// (and drawn once it comes); nil where it is to be its `:name:` as text:
    /// an address that is not the instance's own pictures', or a picture
    /// that could not be had.
    func drawing(of emoji: EmojiSummary) -> EmojiDrawing? {
        guard let url = pictureURL(emoji.imageUrl), !emojiPictures.failed(url) else { return nil }
        return EmojiDrawing(emojiID: emoji.emojiId, name: emoji.name, width: CGFloat(emoji.width), height: CGFloat(emoji.height), animated: emoji.animated, picture: emojiPictures.picture(url))
    }

    /// The emoji with `emojiID` of `serverID` as drawn, when this phone holds
    /// it; nil for one removed or not known yet, which is its text.
    func drawing(emojiID: String, in serverID: String) -> EmojiDrawing? {
        guard !emojiID.isEmpty, let emoji = emoji(of: serverID).first(where: { $0.emojiId == emojiID }) else { return nil }
        return drawing(of: emoji)
    }

    /// The server a conversation is in; empty for a personal one, which has no server's emoji.
    func emojiServer(of channelID: String) -> String {
        guard let channel = channel(channelID), !personalKinds.contains(channel.kind) else { return "" }
        return channel.serverId
    }

    /// The server's emoji a `:` and what follows it in the composer match,
    /// in the core's order; none under two letters.
    func emojiMatches(serverID: String, prefix: String) async -> [EmojiSummary] {
        guard !serverID.isEmpty else { return [] }
        return await client.emojiMatches(serverID: serverID, prefix: prefix)
    }

    /// Reacts to a server's message with one of its emoji, or takes this
    /// account's reaction with it away when it is there (specs/emoji.md 9).
    func reactEmoji(channelID: String, messageID: String, emojiID: String) async throws {
        let mine = messages[channelID]?.first { $0.messageId == messageID }?.reactions.first { $0.emojiId == emojiID }?.me ?? false
        try await client.reactEmoji(channelID: channelID, messageID: messageID, emojiID: emojiID, remove: mine)
        if !mine {
            let serverID = emojiServer(of: channelID)
            if !serverID.isEmpty { FrequentEmoji.remember(FrequentEmoji.custom(serverID: serverID, emojiID: emojiID)) }
        }
    }

    /// Uploads a picture as a new emoji of a server, by the name given; the
    /// snapshot it answers with is held, the emoji among the server's.
    func uploadEmoji(serverID: String, name: String, picture: Data, mediaType: String) async throws {
        take(try await client.uploadEmoji(serverID: serverID, name: EmojiWords.sent(name), picture: picture, mediaType: mediaType))
    }

    func renameEmoji(serverID: String, emojiID: String, name: String) async throws {
        take(try await client.renameEmoji(serverID: serverID, emojiID: emojiID, name: EmojiWords.sent(name)))
    }

    /// Removes one: its picture goes, its name stays in what used it.
    func removeEmoji(serverID: String, emojiID: String) async throws {
        take(try await client.removeEmoji(serverID: serverID, emojiID: emojiID))
        emojiChanged()
    }

    /// A server's emoji changed (`EmojiChanged`, after the snapshot that has
    /// them): the pictures no server lists any more go (specs/emoji.md 7).
    func emojiChanged() {
        let listed = (snapshot?.servers ?? []).flatMap(\.emoji).compactMap { pictureURL($0.imageUrl) }
        emojiPictures.keep(only: Set(listed))
    }
}
