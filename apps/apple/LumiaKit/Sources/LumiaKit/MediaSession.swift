// What the session does for attachments and link cards (specs/media.md):
// what the instance takes, asked once; a file made an attachment and
// uploaded as it is added to the composer, its alt text, and taken out
// again; a link's card while composing; and an attachment's or a card's
// picture fetched, opened and decoded for drawing, kept by blob. Sending
// and editing with them are `send`, `dmSend` and `edit` (`Reads.swift`).
import CoreGraphics
import Foundation
import LumiaFFI

/// A picture as fetched for drawing: the image, one that is not to be
/// decoded (over the bounds, or not a picture at all) and is drawn as a
/// file row, or a fetch that failed.
public enum MediaPicture: Sendable {
    case image(CGImage)
    case undrawable
    case failed(String)

    public var image: CGImage? {
        if case let .image(image) = self { return image }
        return nil
    }
}

/// Pictures decoded for drawing, by blob id, kept while memory allows.
/// The bytes themselves are the core's to keep (its cache directory).
public final class MediaImages: @unchecked Sendable {
    private final class Box { let image: CGImage; init(_ image: CGImage) { self.image = image } }
    private let cache = NSCache<NSString, Box>()

    public init() {
        cache.totalCostLimit = 96 * 1024 * 1024
    }

    public func image(_ key: String) -> CGImage? { cache.object(forKey: key as NSString)?.image }

    public func keep(_ image: CGImage, as key: String) {
        cache.setObject(Box(image), forKey: key as NSString, cost: image.bytesPerRow * image.height)
    }
}

public extension LumiaSession {
    /// Asks the instance once what it takes of attachments.
    func loadUploadLimits() async {
        guard !uploadLimitsAsked else { return }
        uploadLimitsAsked = true
        do {
            uploadLimits = try await client.uploadLimits()
        } catch {
            // Asked again the next time a composer opens.
            uploadLimitsAsked = false
        }
    }

    /// Makes an attachment of a file and uploads it into a conversation:
    /// the draft a send names, or the refusal in words.
    func attach(channelID: String, input: AttachmentInput) async throws -> AttachmentDraft {
        try await client.attach(channelID: channelID, input: input)
    }

    func setAttachmentAlt(draftID: String, alt: String) async throws -> AttachmentDraft {
        try await client.setAttachmentAlt(draftID: draftID, alt: alt)
    }

    /// Takes a waiting attachment out (its blobs off the instance, best effort).
    func discardAttachment(draftID: String) {
        Task { await client.discardAttachment(draftID: draftID) }
    }

    /// The draft's links a composer previews here: none where the
    /// conversation cannot have cards, so nothing waits for one.
    func previewLinks(channelID: String, body: String) -> [String] {
        client.previewLinks(channelID: channelID, body: body)
    }

    /// The card a link has while composing, or none (a page with none, or
    /// one that could not be read, which the composer simply does not show).
    func cardPreview(channelID: String, url: String) async -> CardSummary? {
        (try? await client.cardPreview(channelID: channelID, url: url)) ?? nil
    }

    /// An attachment's bytes, opened.
    func attachmentData(_ blob: BlobSummary) async throws -> Data {
        try await client.attachmentBytes(blob)
    }

    /// An attachment's picture, decoded once its header is within the
    /// bounds: for a message at `MediaInput.tileEdge` (an image's display
    /// picture, a GIF's first frame, a video's poster), for the viewer whole.
    func picture(_ blob: BlobSummary, longest: Int = MediaInput.longestEdge) async -> MediaPicture {
        let key = "\(blob.blobId)@\(longest)"
        if let kept = mediaImages.image(key) { return .image(kept) }
        do {
            let data = try await client.attachmentBytes(blob)
            let decoded = await Task.detached { MediaInput.drawable(data, longest: longest) }.value
            guard let decoded else { return .undrawable }
            mediaImages.keep(decoded, as: key)
            return .image(decoded)
        } catch {
            return .failed(error.lumiaMessage)
        }
    }

    /// The address of a link's embed, made on its press (specs/media.md
    /// 6.6): the core's, a Bluesky post named by handle once Bluesky has
    /// said whose it is, and Twitch's `parent` the instance's host. Throws
    /// an `EmbedUnavailable`, the words to show: Bluesky has no account for
    /// the handle, or gave no answer (`blueskyAddress`).
    func embedAddress(_ embed: EmbedSummary) async throws -> String {
        var url = embed.url
        if url.isEmpty, !embed.blueskyHandle.isEmpty {
            let client = client
            url = try await blueskyAddress(handle: embed.blueskyHandle, rkey: embed.blueskyRkey) { try await client.resolveBlueskyHandle($0) }
        }
        guard let address = EmbedPage.address(url, needsParent: embed.needsParent, instance: client.baseURL) else {
            throw EmbedUnavailable(words: "This can’t be shown here.")
        }
        return address
    }

    /// A card's picture: the instance's copy for a server's card, the
    /// sharer's blob for a personal one; nil for a card without one.
    func cardPicture(_ card: CardSummary) async -> CGImage? {
        if let blob = card.image { return await picture(blob, longest: 800).image }
        guard !card.imageUrl.isEmpty else { return nil }
        let key = "card:\(card.imageUrl)"
        if let kept = mediaImages.image(key) { return kept }
        guard let data = try? await client.cardImage(card.imageUrl),
              let decoded = await Task.detached(operation: { MediaInput.drawable(data, longest: 800) }).value else { return nil }
        mediaImages.keep(decoded, as: key)
        return decoded
    }
}

