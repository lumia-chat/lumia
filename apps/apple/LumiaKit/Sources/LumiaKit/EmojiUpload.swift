// A picture chosen for Upload emoji (specs/emoji.md 10), from the photo
// picker or Files: read off the main actor, and refused over the
// instance's 8 MB before it is read whole or drawn, so a large photo costs
// neither a stall nor a preview for an upload that cannot succeed. What is
// sent is its bytes as chosen; the instance makes the emoji of it.
import Foundation
import UniformTypeIdentifiers

/// A picture chosen to upload: its bytes as chosen, its file's name and
/// type, and its frames to show in the form.
public struct EmojiUploadFile: Identifiable, Hashable, @unchecked Sendable {
    public let id = UUID()
    public let data: Data
    public let fileName: String
    public let mediaType: String
    public let preview: EmojiPicture?

    public static func == (a: EmojiUploadFile, b: EmojiUploadFile) -> Bool { a.id == b.id }
    public func hash(into hasher: inout Hasher) { hasher.combine(id) }

    /// "38 KB", by the system's byte formatter.
    public var size: String { ByteCountFormatter.string(fromByteCount: Int64(data.count), countStyle: .file) }
}

/// What came of a picture chosen: the form's file, or why there is none,
/// in the boards' words.
public enum EmojiUploadRead: Sendable {
    case chosen(EmojiUploadFile)
    case refused(String)
}

public extension EmojiUploadFile {
    /// A picture already in memory (the photo picker's data): refused over
    /// the limit before it is drawn; otherwise named, typed and drawn, the
    /// drawing made off the main actor.
    static func make(data: Data, fileName: String) async -> EmojiUploadRead {
        guard data.count <= ServerSettingsRefusal.largestPicture else { return .refused(EmojiWords.tooLarge) }
        let ext = (fileName as NSString).pathExtension
        let mediaType = UTType(filenameExtension: ext)?.preferredMIMEType ?? "application/octet-stream"
        let preview = await Task.detached { EmojiPicture.preview(data) }.value
        return .chosen(EmojiUploadFile(data: data, fileName: fileName, mediaType: mediaType, preview: preview))
    }

    /// The picture at `url` (the photo picker's copy, or Files' choice,
    /// whose security scope is opened here), read off the main actor: its
    /// size is looked at before anything is read, and no more than the
    /// limit and a byte is ever read, whatever the size said.
    static func read(_ url: URL) async -> EmojiUploadRead {
        let read = await Task.detached { bounded(url, limit: ServerSettingsRefusal.largestPicture) }.value
        switch read {
        case let .bytes(data): return await make(data: data, fileName: url.lastPathComponent)
        case .tooLarge: return .refused(EmojiWords.tooLarge)
        case .unreadable: return .refused(EmojiWords.unreadable)
        }
    }
}

/// A file read to a limit.
private enum Bounded: Sendable {
    case bytes(Data)
    case tooLarge
    case unreadable
}

/// The file's bytes when they come to at most `limit`.
private func bounded(_ url: URL, limit: Int) -> Bounded {
    let scoped = url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    if let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize, size > limit { return .tooLarge }
    guard let handle = try? FileHandle(forReadingFrom: url) else { return .unreadable }
    defer { try? handle.close() }
    guard let data = try? handle.read(upToCount: limit + 1) ?? Data() else { return .unreadable }
    return data.count > limit ? .tooLarge : .bytes(data)
}
