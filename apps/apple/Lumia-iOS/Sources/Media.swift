// Attachments and link cards on the phone (specs/media.md 5 and 6): the
// composer's attach button (the system's photo picker for pictures and
// videos, the document picker for any file), the strip over the field
// with each file's picture, name, size or state and its ×, alt text for a
// picture, and the cards its links will have; a message's attachments
// under its words (pictures at their shape or in a grid, a GIF playing
// unless Reduce Motion is on, a video's poster, an audio player, a file
// row) and its cards, a card with an embed loading it in its place on a
// press; the full-screen picture with pinch zoom, Save to Photos and
// Share; a file shared or saved through the share sheet. Videos and
// recordings play in their rows (`Playback.swift`). Images and Embeds in
// Preferences draw every attachment as a file row and no cards when set
// to links. The kit has
// the rules (`Media.swift`), the platform's decoding (`MediaInput`) and
// the session's side (`MediaSession.swift`).
import AVFoundation
import AVKit
import LumiaFFI
import LumiaKit
import Photos
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers
import WebKit

// MARK: Composing

/// A file from the photo picker, copied out of the picker's hands with its name.
private struct PickedFile: Transferable {
    let url: URL

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .movie) { try copy($0.file) }
        FileRepresentation(importedContentType: .image) { try copy($0.file) }
    }

    private static func copy(_ file: URL) throws -> PickedFile {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-picked/\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let copy = dir.appendingPathComponent(file.lastPathComponent)
        try FileManager.default.copyItem(at: file, to: copy)
        return PickedFile(url: copy)
    }
}

/// What a conversation's composer holds besides its words: the strip of
/// files and the link previews, while the conversation is open.
@MainActor
@Observable
final class ComposerMedia {
    var strip = ComposerAttachments()
    var previews = LinkPreviews()
    /// Each strip file's small picture, by its id.
    var thumbnails: [String: CGImage] = [:]
    var pickingPhotos = false
    var pickingFiles = false
    var photoItems: [PhotosPickerItem] = []
    /// The strip file whose alt text is being written, and the words so far.
    var describing: String?
    var altDraft = ""
    /// What the strip says of itself (full).
    var note = ""

    /// Pictures and videos from the photo picker.
    func add(photos items: [PhotosPickerItem], session: LumiaSession, channelID: String) {
        for item in items {
            let video = item.supportedContentTypes.contains { $0.conforms(to: .movie) }
            let fallback = "\(video ? "Video" : "Photo").\(item.supportedContentTypes.first?.preferredFilenameExtension ?? (video ? "mov" : "jpg"))"
            start(name: fallback, session: session, channelID: channelID) {
                if let picked = try await item.loadTransferable(type: PickedFile.self) {
                    defer { try? FileManager.default.removeItem(at: picked.url.deletingLastPathComponent()) }
                    return try await Self.prepare(url: picked.url, name: picked.url.lastPathComponent, limits: session.uploadLimits)
                }
                guard let data = try await item.loadTransferable(type: Data.self) else { throw MediaFailure("This item could not be read.") }
                return (MediaInput.picture(data: data, name: fallback), MediaInput.preview(data))
            }
        }
    }

    /// Any file from the document picker.
    func add(files urls: [URL], session: LumiaSession, channelID: String) {
        for url in urls {
            start(name: url.lastPathComponent, session: session, channelID: channelID) {
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                return try await Self.prepare(url: url, name: url.lastPathComponent, limits: session.uploadLimits)
            }
        }
    }

    /// A file on disk made ready for the core, off the main thread; one too
    /// large for the instance is refused before it is read.
    private static func prepare(url: URL, name: String, limits: UploadLimits?) async throws -> (AttachmentInput, CGImage?) {
        if let limits, let size = MediaInput.fileSize(url), let words = MediaInput.refusal(size: size, nature: MediaInput.nature(name: name), limits: limits) {
            throw MediaFailure(words)
        }
        return try await Task.detached { try await MediaInput.file(url: url, name: name) }.value
    }

    /// Adds a file to the strip and starts its processing and upload at once.
    private func start(name: String, session: LumiaSession, channelID: String, make: @escaping @MainActor () async throws -> (AttachmentInput, CGImage?)) {
        guard let id = strip.add(name: name) else {
            note = "A message carries at most \(ComposerAttachments.most) files."
            return
        }
        note = ""
        Task {
            do {
                let (input, preview) = try await make()
                if let preview { thumbnails[id] = preview }
                let draft = try await session.attach(channelID: channelID, input: input)
                if strip.items.contains(where: { $0.id == id }) {
                    strip.uploaded(id, draft)
                } else {
                    session.discardAttachment(draftID: draft.draftId)
                }
            } catch let failure as MediaFailure {
                strip.failed(id, failure.words)
            } catch {
                strip.failed(id, MediaWords.refusal(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage))
            }
        }
    }

    /// The ×.
    func remove(_ id: String, session: LumiaSession) {
        if let draft = strip.remove(id) { session.discardAttachment(draftID: draft) }
        thumbnails[id] = nil
        note = ""
    }

    func describe(_ item: ComposerAttachment) {
        describing = item.id
        altDraft = item.draft?.alt ?? ""
    }

    func saveAlt(session: LumiaSession) {
        guard let id = describing, let draft = strip.items.first(where: { $0.id == id })?.draft else { return }
        describing = nil
        let alt = altDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        Task {
            if let described = try? await session.setAttachmentAlt(draftID: draft.draftId, alt: alt) { strip.updated(described) }
        }
    }

    /// Typing has paused: the draft's links that may have cards, each asked for once.
    func links(in draft: String, session: LumiaSession, channelID: String) {
        for url in previews.update(links: session.previewLinks(channelID: channelID, body: draft)) {
            Task {
                let card = await session.cardPreview(channelID: channelID, url: url)
                previews.arrived(url, card: card)
            }
        }
    }

    /// What a send carries: the drafts, and the links whose cards were kept.
    func outgoing(body: String) -> (attachments: [String], cards: [String]) {
        (strip.draftIDs, previews.kept(in: cardLinks(body: body)))
    }

    /// Sent: the drafts and the previews go.
    func sent() {
        strip.sent()
        let left = Set(strip.items.map(\.id))
        thumbnails = thumbnails.filter { left.contains($0.key) }
        previews.reset()
    }

    /// The conversation was left or changed: whatever waits is discarded.
    func reset(session: LumiaSession) {
        for draft in strip.clear() { session.discardAttachment(draftID: draft) }
        thumbnails = [:]
        previews.reset()
        note = ""
    }
}

/// A file refused before it reached the core, in words.
struct MediaFailure: Error {
    let words: String
    init(_ words: String) { self.words = words }
}

/// The composer's attach button: the photo picker or any file.
struct AttachButton: View {
    let media: ComposerMedia

    var body: some View {
        Menu {
            Button(MediaWords.photos, systemImage: "photo.on.rectangle") { media.pickingPhotos = true }
            Button(MediaWords.files, systemImage: "folder") { media.pickingFiles = true }
        } label: {
            Image(systemName: "plus")
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(LumiaTokens.inkSecondary.color)
                .frame(width: 36, height: 36)
                .contentShape(Rectangle())
        }
        .disabled(media.strip.room == 0)
        .accessibilityLabel(MediaWords.attach)
        .accessibilityIdentifier("attach")
    }
}

extension View {
    /// The pickers the attach button opens and the alt text alert, over a
    /// conversation, and what is in the strip discarded as it changes.
    func composerMedia(_ media: ComposerMedia, session: LumiaSession, channelID: String) -> some View {
        @Bindable var media = media
        return self
            .photosPicker(isPresented: $media.pickingPhotos, selection: $media.photoItems, maxSelectionCount: max(1, media.strip.room), matching: .any(of: [.images, .videos]), preferredItemEncoding: .current)
            .onChange(of: media.photoItems) { _, items in
                guard !items.isEmpty else { return }
                media.photoItems = []
                media.add(photos: items, session: session, channelID: channelID)
            }
            .fileImporter(isPresented: $media.pickingFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
                if case let .success(urls) = result { media.add(files: Array(urls.prefix(media.strip.room)), session: session, channelID: channelID) }
            }
            .alert("Alt text", isPresented: Binding(get: { media.describing != nil }, set: { if !$0 { media.describing = nil } })) {
                TextField("Description", text: $media.altDraft, axis: .vertical)
                Button("Cancel", role: .cancel) { media.describing = nil }
                Button("Save") { media.saveAlt(session: session) }
            } message: {
                Text(MediaWords.altPrompt)
            }
            .task(id: channelID) { await session.loadUploadLimits() }
            // Another conversation: what waited here is discarded. A
            // conversation left by going back leaves its drafts to the
            // instance, which removes an upload nothing names within a day.
            .onChange(of: channelID) { _, _ in media.reset(session: session) }
    }

    /// Reads the draft's links once typing pauses for half a second.
    func linkPreviews(_ media: ComposerMedia, draft: String, session: LumiaSession, channelID: String) -> some View {
        task(id: draft) {
            try? await Task.sleep(for: LinkPreviews.pause)
            guard !Task.isCancelled else { return }
            media.links(in: draft, session: session, channelID: channelID)
        }
    }
}

/// Over the field: the cards the links will have, each with its ×, then
/// the strip of files.
struct ComposerMediaBars: View {
    let media: ComposerMedia
    let session: LumiaSession

    var body: some View {
        if !media.previews.shown.isEmpty {
            VStack(spacing: LumiaTokens.space6) {
                ForEach(media.previews.shown, id: \.url) { card in
                    HStack(alignment: .top, spacing: LumiaTokens.space8) {
                        LinkCardView(card: card, session: session, compact: true)
                        RemoveButton(label: "Remove the card for \(card.host)") { media.previews.dismiss(card.url) }
                    }
                }
            }
        }
        if !media.strip.isEmpty || !media.note.isEmpty {
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(alignment: .top, spacing: LumiaTokens.space8) {
                        ForEach(media.strip.items) { item in
                            StripItem(item: item, thumbnail: media.thumbnails[item.id], remove: { media.remove(item.id, session: session) }, describe: { media.describe(item) })
                        }
                    }
                    .padding(.top, LumiaTokens.space6)
                }
                if !media.note.isEmpty { LumiaErrorText(media.note) }
            }
        }
    }
}

/// One file in the strip: its picture (or what stands for it), its name,
/// its size or state, its ×, and for a picture its alt text.
private struct StripItem: View {
    let item: ComposerAttachment
    let thumbnail: CGImage?
    let remove: () -> Void
    let describe: () -> Void

    private static let side: CGFloat = 64
    private static let width: CGFloat = 112

    private var kind: AttachmentKind { AttachmentKind(item.draft?.kind ?? "file") }
    private var picture: Bool { item.draft.map { AttachmentKind($0.kind) == .image || AttachmentKind($0.kind) == .gif } ?? false }

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space4) {
            ZStack(alignment: .topTrailing) {
                ZStack {
                    RoundedRectangle(cornerRadius: LumiaTokens.radius10).fill(LumiaTokens.inset.color)
                    if let thumbnail {
                        Image(decorative: thumbnail, scale: 1).resizable().scaledToFill()
                    } else if let draft = item.draft, let placeholder = placeholderImage(placeholder: draft.placeholder).flatMap(MediaInput.image) {
                        Image(decorative: placeholder, scale: 1).resizable().scaledToFill()
                    } else {
                        Image(systemName: item.draft.map { MediaWords.symbol(kind: AttachmentKind($0.kind), mediaType: $0.mediaType, name: $0.name) } ?? "doc")
                            .font(.system(size: 22)).foregroundStyle(LumiaTokens.inkMuted.color)
                    }
                    if item.uploading {
                        LumiaTokens.scrim.color
                        ProgressView().tint(.white)
                    } else if item.failure != nil {
                        LumiaTokens.scrim.color
                        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(LumiaTokens.danger.color)
                    }
                }
                .frame(width: Self.side, height: Self.side)
                .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius10))
                RemoveButton(label: "Remove \(item.shownName)", action: remove)
                    .offset(x: LumiaTokens.space8, y: -LumiaTokens.space6)
            }
            Text(item.shownName).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1).truncationMode(.middle)
            Text(MediaWords.state(item))
                .font(LumiaTokens.textMeta.font)
                .foregroundStyle(item.failure == nil ? LumiaTokens.inkMuted.color : LumiaTokens.danger.color)
                .lineLimit(3)
                .fixedSize(horizontal: false, vertical: true)
            if picture, let draft = item.draft {
                Button(draft.alt.isEmpty ? "Add alt text" : "Alt text ✓", action: describe)
                    .font(LumiaTokens.textMeta.font)
                    .buttonStyle(.lumiaLink)
            }
        }
        .frame(width: Self.width, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(item.shownName), \(MediaWords.state(item))")
        .accessibilityValue(item.draft.map { $0.alt.isEmpty ? "" : "Alt text: \($0.alt)" } ?? "")
        .accessibilityActions {
            Button("Remove", action: remove)
            if picture { Button(item.draft?.alt.isEmpty == false ? "Edit alt text" : "Add alt text", action: describe) }
        }
    }
}

/// A round × over a strip file or beside a card.
struct RemoveButton: View {
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "xmark")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(LumiaTokens.ink.color)
                .frame(width: 22, height: 22)
                .background(LumiaTokens.raised.color, in: Circle())
                .overlay(Circle().stroke(LumiaTokens.line.color))
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

// MARK: A message's media

/// What a message carries, opened: a picture in the viewer, anything else
/// through the share sheet. A video or a recording plays in its row instead.
enum MediaOpen: Identifiable {
    case picture(AttachmentSummary)
    case file(AttachmentSummary)

    var id: String {
        switch self {
        case let .picture(a): "picture-\(a.attachmentId)"
        case let .file(a): "file-\(a.attachmentId)"
        }
    }

    /// Opened full screen, rather than in the share sheet.
    var fullScreen: Bool {
        if case .file = self { return false }
        return true
    }

    /// How an attachment opens by its kind.
    static func of(_ a: AttachmentSummary) -> MediaOpen {
        switch AttachmentKind(a.kind) {
        case .image, .gif: .picture(a)
        case .video, .audio, .file: .file(a)
        }
    }
}

/// While a message is edited: the ×s on what it carries, and what they took off.
struct MediaEditing {
    let removed: MediaEdit
    let removeAttachment: (String) -> Void
    let removeCard: (String) -> Void
}

/// A message's attachments and cards under its words: the pictures (and
/// videos by their posters) at their shape or in a grid, then audio and
/// files as rows, then the cards. Preferences may draw every attachment
/// as a row and no cards. A long press anywhere on them is `hold`, the
/// message's actions, as on its words; a tap still opens, plays or shares.
/// A video or a recording plays in place, and a card with an embed loads
/// it in its place, keyed by `messageID` so one link in two messages is
/// two embeds.
struct MessageMedia: View {
    @Environment(Preferences.self) private var preferences
    let messageID: String
    let attachments: [AttachmentSummary]
    let cards: [CardSummary]
    let session: LumiaSession
    let open: (MediaOpen) -> Void
    var editing: MediaEditing?
    /// The message's actions: nil while it is edited, or where it has none.
    var hold: (() -> Void)?

    private static let gap = LumiaTokens.space2

    private var shown: [AttachmentSummary] { attachments.filter { !(editing?.removed.removedAttachments.contains($0.attachmentId) ?? false) } }
    private var shownCards: [CardSummary] {
        guard preferences.embeds == .shown || editing != nil else { return [] }
        return cards.filter { !(editing?.removed.removedCards.contains($0.url) ?? false) }
    }

    var body: some View {
        let asRows = preferences.images == .link
        // One the server let go is a row whatever it was, and opens nothing.
        let visual = asRows ? [] : shown.filter { !$0.expired && AttachmentKind($0.kind).visual }
        let rows = asRows ? shown : shown.filter { $0.expired || !AttachmentKind($0.kind).visual }
        VStack(alignment: .leading, spacing: LumiaTokens.space6) {
            if !visual.isEmpty {
                PictureGrid(sizes: visual.map { ($0.width, $0.height) }, gap: Self.gap) {
                    ForEach(visual, id: \.attachmentId) { a in
                        removable(a.attachmentId) {
                            AttachmentTile(attachment: a, messageID: messageID, session: session, single: visual.count == 1, hold: hold) { open(.of(a)) }
                        }
                    }
                }
            }
            ForEach(rows, id: \.attachmentId) { a in
                removable(a.attachmentId) {
                    if a.expired {
                        ExpiredRow(attachment: a, hold: hold)
                    } else if AttachmentKind(a.kind) == .audio, !asRows {
                        AudioRow(attachment: a, messageID: messageID, session: session, hold: hold) { open(.file(a)) }
                    } else if AttachmentKind(a.kind) == .video {
                        // Images as links: a video's row plays it under itself on a press.
                        VideoFileRow(attachment: a, messageID: messageID, session: session, hold: hold)
                    } else {
                        FileRow(attachment: a, asFile: asRows, hold: hold) { open(.of(a)) }
                    }
                }
            }
            ForEach(shownCards, id: \.url) { card in
                if let editing {
                    HStack(alignment: .top, spacing: LumiaTokens.space8) {
                        LinkCardView(card: card, session: session, hold: hold)
                        RemoveButton(label: "Remove the card for \(card.host)") { editing.removeCard(card.url) }
                    }
                } else {
                    LinkCardView(card: card, session: session, hold: hold, embedKey: "\(messageID)|\(card.url)")
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder private func removable(_ id: String, @ViewBuilder _ content: () -> some View) -> some View {
        if let editing {
            content().overlay(alignment: .topTrailing) {
                RemoveButton(label: "Remove this attachment") { editing.removeAttachment(id) }
            }
        } else {
            content()
        }
    }
}

/// Lays a message's pictures out in its column: one at its own shape, no
/// wider than the column or its pixels and at most 400 high; several as a
/// grid of squares (`MediaLayout`).
struct PictureGrid: Layout {
    let sizes: [(UInt32, UInt32)]
    let gap: CGFloat

    private func frames(width: CGFloat) -> [CGRect] {
        if sizes.count == 1 {
            return [CGRect(origin: .zero, size: MediaLayout.single(width: sizes[0].0, height: sizes[0].1, column: width))]
        }
        return MediaLayout.tiles(count: sizes.count, column: width, gap: gap)
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 300
        let all = frames(width: width)
        return CGSize(width: all.map(\.maxX).max() ?? 0, height: all.map(\.maxY).max() ?? 0)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for (view, frame) in zip(subviews, frames(width: bounds.width)) {
            view.place(at: CGPoint(x: bounds.minX + frame.minX, y: bounds.minY + frame.minY), proposal: ProposedViewSize(frame.size))
        }
    }
}

/// An attachment's placeholder, drawn until its thumbnail arrives.
private func placeholderPicture(_ a: AttachmentSummary) -> CGImage? {
    placeholderImage(placeholder: a.placeholder).flatMap(MediaInput.image)
}

/// A picture, GIF or video in a message: its placeholder, then its
/// thumbnail (the video's poster) with a play symbol over a video and the
/// GIF badge over a GIF that is still; a GIF plays in place unless Reduce
/// Motion is on, when a tap plays it first. A video pressed plays in its
/// place with the system's controls and pauses when it scrolls away. A
/// picture whose header is over the bounds is drawn as what it is, a file.
private struct AttachmentTile: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(MediaPlayback.self) private var playback: MediaPlayback?
    let attachment: AttachmentSummary
    let messageID: String
    let session: LumiaSession
    let single: Bool
    let hold: (() -> Void)?
    let open: () -> Void
    @State private var picture: CGImage?
    @State private var placeholder: CGImage?
    @State private var undrawable = false
    @State private var animation: UIImage?
    @State private var playing = false

    private var kind: AttachmentKind { AttachmentKind(attachment.kind) }

    var body: some View {
        // One view around both, so the poster giving way to the player is
        // not its going out of sight.
        ZStack {
            if kind == .video, let playback, let player = playback.players[attachment.attachmentId] {
                InlineVideo(player: player, key: attachment.attachmentId, playback: playback, hold: hold)
                    .clipShape(RoundedRectangle(cornerRadius: single ? LumiaTokens.radius12 : LumiaTokens.radius6))
                    .accessibilityLabel(MediaWords.label(attachment))
            } else {
                tile
            }
        }
        .onDisappear { if kind == .video { playback?.away(attachment.attachmentId) } }
    }

    private var tile: some View {
        ZStack {
            LumiaTokens.inset.color
            if undrawable {
                VStack(spacing: LumiaTokens.space4) {
                    Image(systemName: "doc").font(.system(size: 22))
                    Text(attachment.name).font(LumiaTokens.textMeta.font).lineLimit(2).multilineTextAlignment(.center)
                }
                .foregroundStyle(LumiaTokens.inkMuted.color)
                .padding(LumiaTokens.space8)
            } else if let animation, playing || !reduceMotion {
                AnimatedImage(image: animation)
            } else if let picture {
                Image(decorative: picture, scale: 1).resizable().scaledToFill()
            } else if let placeholder {
                Image(decorative: placeholder, scale: 1).resizable().scaledToFill()
            } else if kind == .video {
                Image(systemName: "film").font(.system(size: 24)).foregroundStyle(LumiaTokens.inkMuted.color)
            }
            if kind == .video {
                Group {
                    if playback?.loading.contains(attachment.attachmentId) == true {
                        ProgressView().tint(.white)
                    } else {
                        Image(systemName: "play.fill").font(.system(size: 18, weight: .semibold))
                    }
                }
                .foregroundStyle(.white)
                .frame(width: 44, height: 44)
                .background(.black.opacity(0.55), in: Circle())
            }
        }
        .overlay(alignment: .bottomLeading) {
            if kind == .gif, reduceMotion, !playing { MediaBadge(text: MediaWords.gifBadge) }
            if kind == .video, let failure = playback?.failures[attachment.attachmentId] {
                MediaBadge(text: failure)
            } else if kind == .video, attachment.durationMs > 0 {
                MediaBadge(text: MediaWords.duration(ms: attachment.durationMs))
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: single ? LumiaTokens.radius12 : LumiaTokens.radius6))
        .overlay(RoundedRectangle(cornerRadius: single ? LumiaTokens.radius12 : LumiaTokens.radius6).stroke(LumiaTokens.line.color))
        .pressed(hold: hold) {
            // Reduce Motion: the first tap plays a still GIF in place.
            if kind == .gif, reduceMotion, !playing, animation != nil {
                playing = true
            } else if kind == .video, let playback {
                playback.press(attachment, in: messageID, session: session)
            } else {
                open()
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(MediaWords.label(attachment, asFile: undrawable))
        .accessibilityHint(kind == .video ? "Plays it here" : "")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("attachment")
        .messageActions(hold)
        .task(id: attachment.attachmentId) { await load() }
    }

    private func load() async {
        placeholder = placeholderPicture(attachment)
        guard let blob = attachment.thumbnail ?? (kind == .image ? attachment.main : nil) else { return }
        switch await session.picture(blob, longest: 800) {
        case let .image(image): picture = image
        case .undrawable: undrawable = true
        case .failed: break
        }
        if kind == .gif, let data = try? await session.attachmentData(attachment.main) {
            animation = await Task.detached { AnimatedImage.frames(data) }.value
        }
    }
}

/// A GIF's frames, playing.
private struct AnimatedImage: UIViewRepresentable {
    let image: UIImage

    func makeUIView(context: Context) -> UIImageView {
        let view = UIImageView(image: image)
        view.contentMode = .scaleAspectFill
        view.clipsToBounds = true
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        view.startAnimating()
        return view
    }

    func updateUIView(_ view: UIImageView, context: Context) {
        if view.image !== image { view.image = image }
        view.startAnimating()
    }

    /// The frames of a GIF someone sent, each at most 400 px, at most 200
    /// of them; none when its header is over the bounds.
    static func frames(_ data: Data) -> UIImage? {
        guard attachmentDrawable(bytes: data), let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let count = min(CGImageSourceGetCount(source), 200)
        guard count > 1 else { return nil }
        var frames: [UIImage] = []
        var seconds = 0.0
        let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 400, kCGImageSourceCreateThumbnailWithTransform: true]
        for n in 0..<count {
            guard let frame = CGImageSourceCreateThumbnailAtIndex(source, n, options as CFDictionary) else { continue }
            frames.append(UIImage(cgImage: frame))
            let props = CGImageSourceCopyPropertiesAtIndex(source, n, nil) as? [CFString: Any]
            let gif = props?[kCGImagePropertyGIFDictionary] as? [CFString: Any]
            let delay = (gif?[kCGImagePropertyGIFUnclampedDelayTime] as? Double) ?? (gif?[kCGImagePropertyGIFDelayTime] as? Double) ?? 0.1
            seconds += delay < 0.02 ? 0.1 : delay
        }
        return frames.isEmpty ? nil : UIImage.animatedImage(with: frames, duration: seconds)
    }
}

/// A small label over a picture's corner: "GIF", a video's length.
private struct MediaBadge: View {
    let text: String
    var body: some View {
        Text(text)
            .font(LumiaTokens.textTag.font)
            .foregroundStyle(.white)
            .padding(.horizontal, LumiaTokens.space6)
            .padding(.vertical, LumiaTokens.space2)
            .background(.black.opacity(0.6), in: RoundedRectangle(cornerRadius: LumiaTokens.radius5))
            .padding(LumiaTokens.space6)
            .accessibilityHidden(true)
    }
}

/// A file as a row: an icon for its kind, its name, its size (and a
/// length), and Save, which opens the share sheet. Tapped, it opens as
/// what it is: a picture in the viewer, a video in the player.
struct FileRow: View {
    let attachment: AttachmentSummary
    var asFile = false
    var hold: (() -> Void)?
    /// Said in place of its size: a video loading, or why it cannot play.
    var note: String?
    let open: () -> Void

    private var kind: AttachmentKind { AttachmentKind(attachment.kind) }
    private var pdf: Bool { MediaWords.symbol(kind: kind, mediaType: attachment.mediaType, name: attachment.name) == "doc.richtext" }

    var body: some View {
        Group {
            HStack(spacing: LumiaTokens.space10) {
                Image(systemName: MediaWords.symbol(kind: kind, mediaType: attachment.mediaType, name: attachment.name))
                    .font(.system(size: 18))
                    .foregroundStyle(pdf ? LumiaTokens.filePdfInk.color : LumiaTokens.inkSecondary.color)
                    .frame(width: 40, height: 40)
                    .background(pdf ? LumiaTokens.filePdf.color : LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    Text(attachment.name).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1).truncationMode(.middle)
                    Text(note ?? MediaWords.detail(kind: kind, sizeBytes: attachment.sizeBytes, durationMs: attachment.durationMs))
                        .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
                }
                Spacer(minLength: 0)
                Image(systemName: kind == .video ? "play.fill" : kind.visual ? "arrow.up.left.and.arrow.down.right" : "square.and.arrow.down")
                    .font(.system(size: 15))
                    .foregroundStyle(LumiaTokens.inkSecondary.color)
                    .frame(width: 32, height: 32)
            }
            .padding(LumiaTokens.space8)
            .frame(maxWidth: 360, alignment: .leading)
            .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
        }
        .pressed(hold: hold, open)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(MediaWords.label(attachment, asFile: asFile || kind == .file))
        .accessibilityHint(kind == .video ? "Plays it here" : kind.visual ? "Opens it" : "Opens the share sheet to save or share it")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("attachment")
        .messageActions(hold)
    }
}

/// An attachment its server let go (specs/media.md 4.5): a file row with
/// its name and "No longer available", fetching nothing and opening
/// nothing; a long press still opens its message's actions.
private struct ExpiredRow: View {
    let attachment: AttachmentSummary
    var hold: (() -> Void)?

    var body: some View {
        let kind = AttachmentKind(attachment.kind)
        HStack(spacing: LumiaTokens.space10) {
            Image(systemName: MediaWords.symbol(kind: kind, mediaType: attachment.mediaType, name: attachment.name))
                .font(.system(size: 18))
                .foregroundStyle(LumiaTokens.inkMuted.color)
                .frame(width: 40, height: 40)
                .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                Text(attachment.name).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1).truncationMode(.middle)
                Text(MediaWords.expired).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
            }
            Spacer(minLength: 0)
        }
        .padding(LumiaTokens.space8)
        .frame(maxWidth: 360, alignment: .leading)
        .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
        .contentShape(Rectangle())
        .onLongPressGesture(minimumDuration: MediaPress.hold) { hold?() }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(MediaWords.label(attachment))
        .accessibilityIdentifier("attachment")
        .messageActions(hold)
    }
}

/// An audio file: play and pause in place, its name and length (where it
/// is while it plays), and Save. It plays on while scrolled away, until
/// another starts or the conversation is left (`MediaPlayback`).
private struct AudioRow: View {
    @Environment(MediaPlayback.self) private var playback: MediaPlayback?
    let attachment: AttachmentSummary
    let messageID: String
    let session: LumiaSession
    let hold: (() -> Void)?
    let share: () -> Void

    private var key: String { attachment.attachmentId }

    var body: some View {
        let playing = playback?.isPlaying(key) ?? false
        let loading = playback?.loading.contains(key) ?? false
        let error = playback?.failures[key] ?? ""
        let started = playback?.players[key] != nil
        HStack(spacing: LumiaTokens.space10) {
            Group {
                if loading { ProgressView() } else { Image(systemName: playing ? "pause.fill" : "play.fill").font(.system(size: 16)) }
            }
            .foregroundStyle(LumiaTokens.onAccent.color)
            .frame(width: 40, height: 40)
            .background(LumiaTokens.accent.color, in: Circle())
            .pressed(hold: hold) { playback?.press(attachment, in: messageID, session: session) }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(playing ? "Pause" : "Play")
            .accessibilityValue(attachment.name)
            .accessibilityAddTraits(.isButton)
            .messageActions(hold)
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                Text(attachment.name).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1).truncationMode(.middle)
                Text(!error.isEmpty ? error : started ? MediaWords.position(ms: playback?.positions[key] ?? 0, of: attachment.durationMs) : MediaWords.detail(kind: .audio, sizeBytes: attachment.sizeBytes, durationMs: attachment.durationMs))
                    .font(LumiaTokens.textMeta.font).foregroundStyle(error.isEmpty ? LumiaTokens.inkMuted.color : LumiaTokens.danger.color)
                    .monospacedDigit()
            }
            .accessibilityElement(children: .combine)
            .messageActions(hold)
            Spacer(minLength: 0)
            Image(systemName: "square.and.arrow.down").font(.system(size: 15)).foregroundStyle(LumiaTokens.inkSecondary.color).frame(width: 32, height: 32)
                .pressed(hold: hold, share)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Save \(attachment.name)")
                .accessibilityAddTraits(.isButton)
                .messageActions(hold)
        }
        .padding(LumiaTokens.space8)
        .frame(maxWidth: 360, alignment: .leading)
        .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
        // Between its buttons the row's own ground takes the long press too.
        .contentShape(Rectangle())
        .onLongPressGesture(minimumDuration: MediaPress.hold) { hold?() }
    }
}

/// A video drawn as a file row (Images as links): pressed, it plays under
/// the row at its shape, and pauses when the row scrolls away.
private struct VideoFileRow: View {
    @Environment(MediaPlayback.self) private var playback: MediaPlayback?
    let attachment: AttachmentSummary
    let messageID: String
    let session: LumiaSession
    let hold: (() -> Void)?

    private var key: String { attachment.attachmentId }

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space6) {
            FileRow(attachment: attachment, asFile: true, hold: hold, note: playback?.loading.contains(key) == true ? "Loading…" : playback?.failures[key]) {
                playback?.press(attachment, in: messageID, session: session)
            }
            if let playback, let player = playback.players[key] {
                PictureGrid(sizes: [(attachment.width, attachment.height)], gap: 0) {
                    InlineVideo(player: player, key: key, playback: playback, hold: hold)
                        .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                        .accessibilityLabel(MediaWords.label(attachment))
                }
            }
        }
        .onDisappear { playback?.away(key) }
    }
}

/// A link's card: the site's host as the link names it (never what the
/// page says it is), the title, the description, the picture with a play
/// symbol for a video page; tapped, the link opens in the browser. A link
/// with an embed (specs/media.md 6.6, the core's `embedFor` of the link,
/// never the card) has a play button or "Show post" instead, and its press
/// loads the embed in the card's place; nothing is fetched from the site
/// before it.
struct LinkCardView: View {
    @Environment(\.openURL) private var openURL
    @Environment(MediaPlayback.self) private var playback: MediaPlayback?
    let card: CardSummary
    let session: LumiaSession
    var compact = false
    /// The message's actions, on a long press; none in the composer's preview.
    var hold: (() -> Void)?
    /// Where its embed is kept while shown: none in the composer, nor while
    /// its message is edited, where a card is only a card.
    var embedKey: String?
    @State private var picture: CGImage?

    private var bar: Color {
        guard card.color.count == 7, card.color.hasPrefix("#"), let rgb = UInt32(card.color.dropFirst(), radix: 16) else { return LumiaTokens.lineStrong.color }
        return Color(.sRGB, red: Double(rgb >> 16 & 0xFF) / 255, green: Double(rgb >> 8 & 0xFF) / 255, blue: Double(rgb & 0xFF) / 255)
    }

    private var hasPicture: Bool { !card.imageUrl.isEmpty || card.image != nil }
    /// A wide picture (a page's banner) is drawn whole under the words; a
    /// squarer one, or any in the composer's preview, small at their side.
    private var wide: Bool { !compact && card.imageHeight > 0 && Double(card.imageWidth) / Double(card.imageHeight) >= 1.4 }

    /// The link's embed, while its press loads one here.
    private var embed: EmbedSummary? {
        guard let embedKey, let playback, !playback.unavailable.contains(embedKey) else { return nil }
        return embedFor(url: card.url)
    }

    var body: some View {
        if let embedKey, let playback, playback.shows(embedKey), let web = playback.webView, let embed = embedFor(url: card.url) {
            EmbedFrame(card: card, embed: embed, key: embedKey, web: web, playback: playback, hold: hold)
        } else {
            cardView
        }
    }

    private var cardView: some View {
        let embed = embed
        let busy = embedKey.map { playback?.loading.contains($0) == true } ?? false
        let failure = embedKey.flatMap { playback?.failures[$0] }
        return Group {
            VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                HStack(alignment: .top, spacing: LumiaTokens.space10) {
                    VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                        Text(card.host).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1)
                        Text(card.title).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.link.color).lineLimit(compact ? 1 : 2)
                        if !card.description.isEmpty {
                            Text(card.description).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(compact ? 1 : 3)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    if hasPicture, !wide {
                        cardPicture(play: nil).frame(width: compact ? 44 : 72, height: compact ? 44 : 72)
                    }
                }
                if hasPicture, wide {
                    cardPicture(play: embed.flatMap { $0.kind == "post" ? nil : $0 })
                        .aspectRatio(CGFloat(card.imageWidth) / CGFloat(card.imageHeight), contentMode: .fit)
                        .frame(maxHeight: 200, alignment: .leading)
                }
                if let embed {
                    EmbedButton(embed: embed, busy: busy)
                }
                if let failure {
                    Text(failure).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.danger.color)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.leading, LumiaTokens.space10 + 3)
            .padding([.vertical, .trailing], LumiaTokens.space10)
            .frame(maxWidth: 400, alignment: .leading)
            .background(alignment: .leading) {
                bar.frame(width: 3)
            }
            .background(LumiaTokens.surface.color)
            .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius10))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius10).stroke(LumiaTokens.line.color))
        }
        .pressed(hold: hold) {
            if let embed, let embedKey, let playback {
                playback.show(embed, key: embedKey, title: EmbedWords.title(embed, cardTitle: card.title), session: session)
            } else if let url = URL(string: card.url) {
                openURL(url)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(embed.map { EmbedWords.press($0, title: card.title) } ?? MediaWords.cardLabel(card))
        .accessibilityValue([card.description, busy ? EmbedWords.finding : "", failure ?? ""].filter { !$0.isEmpty }.joined(separator: ", "))
        .accessibilityHint(embed == nil ? "Opens the link in the browser" : "")
        .accessibilityAddTraits(embed == nil ? .isLink : .isButton)
        .accessibilityIdentifier("card")
        .messageActions(hold)
        .task(id: card.url) {
            if hasPicture { picture = await session.cardPicture(card) }
        }
    }

    /// The page's picture, with a play symbol over a video page's, and a
    /// larger one over a player's or a site's audio, whose press plays it.
    private func cardPicture(play embed: EmbedSummary?) -> some View {
        ZStack {
            LumiaTokens.inset.color
            if let picture { Image(decorative: picture, scale: 1).resizable().scaledToFill() }
            if embed != nil {
                Image(systemName: "play.fill").font(.system(size: 18, weight: .semibold)).foregroundStyle(.white)
                    .frame(width: 44, height: 44).background(.black.opacity(0.55), in: Circle())
            } else if card.video {
                Image(systemName: "play.fill").font(.system(size: 12, weight: .semibold)).foregroundStyle(.white)
                    .frame(width: 28, height: 28).background(.black.opacity(0.55), in: Circle())
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius8))
    }
}

/// A card's play button or "Show post"; a spinner and "Finding the post…"
/// while a Bluesky handle is resolved. The card as a whole is the press.
private struct EmbedButton: View {
    let embed: EmbedSummary
    let busy: Bool

    var body: some View {
        HStack(spacing: LumiaTokens.space6) {
            if busy {
                ProgressView().controlSize(.small)
                Text(EmbedWords.finding)
            } else {
                Image(systemName: embed.kind == "post" ? "text.bubble" : "play.fill").font(.system(size: 12, weight: .semibold))
                Text(EmbedWords.button(embed))
            }
        }
        .font(LumiaTokens.textMeta.font)
        .foregroundStyle(busy ? LumiaTokens.inkSecondary.color : LumiaTokens.onAccent.color)
        .padding(.horizontal, LumiaTokens.space10)
        .padding(.vertical, LumiaTokens.space6)
        .background(busy ? LumiaTokens.inset.color : LumiaTokens.accent.color, in: Capsule())
        .accessibilityHidden(true)
    }
}

/// An embed in its card's place: a bar with the site's name, Open in
/// browser and Close (back to the card), and the site's own embed under
/// it at its size. The bar takes the long press for the message's
/// actions, since the embed's own touches are the site's. Out of sight, a
/// player or a post goes back to its card; a site's audio plays on.
private struct EmbedFrame: View {
    @Environment(\.openURL) private var openURL
    let card: CardSummary
    let embed: EmbedSummary
    let key: String
    let web: WKWebView
    let playback: MediaPlayback
    let hold: (() -> Void)?

    var body: some View {
        EmbedBox(embed: embed, reported: playback.heights[key]) {
            HStack(spacing: LumiaTokens.space4) {
                Text(embed.providerName).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                    .onLongPressGesture(minimumDuration: MediaPress.hold) { hold?() }
                    .accessibilityHidden(true)
                Button {
                    if let url = URL(string: card.url) { openURL(url) }
                } label: {
                    Image(systemName: "safari").font(.system(size: 14)).frame(width: 32, height: 28)
                }
                .accessibilityLabel("Open in browser")
                Button {
                    playback.close(key)
                } label: {
                    Image(systemName: "xmark").font(.system(size: 12, weight: .semibold)).frame(width: 32, height: 28)
                }
                .accessibilityLabel("Close \(embed.providerName)")
            }
            .buttonStyle(.plain)
            .foregroundStyle(LumiaTokens.inkSecondary.color)
            .padding(.leading, LumiaTokens.space10)
            .padding(.vertical, LumiaTokens.space2)
            EmbedHost(web: web)
                .accessibilityIdentifier("embed")
        }
        .background(LumiaTokens.surface.color)
        .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius10))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius10).stroke(LumiaTokens.line.color))
        .onDisappear { playback.away(key) }
    }
}

/// A piece of a message's media pressed: a tap does what the piece does,
/// and a long press is the message's actions, as on its words. They are
/// one view's gestures, so only one of them happens: a long press that
/// opened the actions does not also open the picture when the finger
/// lifts, as a button's tap would however long it was held.
enum MediaPress {
    /// As long as the row's own long press.
    static let hold = 0.4
}

extension View {
    func pressed(hold: (() -> Void)?, _ tap: @escaping () -> Void) -> some View {
        contentShape(Rectangle())
            .onTapGesture(perform: tap)
            .onLongPressGesture(minimumDuration: MediaPress.hold) { hold?() }
    }

    /// The message's actions for VoiceOver on a piece that is a stop of
    /// its own, as its row has them.
    func messageActions(_ hold: (() -> Void)?) -> some View {
        accessibilityActions {
            if let hold { Button("Message actions", action: hold) }
        }
    }
}

// MARK: Opening

extension View {
    /// What a conversation's attachments open into: the picture viewer full
    /// screen, the share sheet for anything else.
    func mediaOpening(_ opening: Binding<MediaOpen?>, session: LumiaSession) -> some View {
        let full = Binding<MediaOpen?>(get: { opening.wrappedValue?.fullScreen == true ? opening.wrappedValue : nil }, set: { if $0 == nil { opening.wrappedValue = nil } })
        let shared = Binding<MediaOpen?>(get: { opening.wrappedValue?.fullScreen == false ? opening.wrappedValue : nil }, set: { if $0 == nil { opening.wrappedValue = nil } })
        return fullScreenCover(item: full) { (open: MediaOpen) in
            switch open {
            case let .picture(a): PictureViewer(attachment: a, session: session)
            case .file: EmptyView()
            }
        }
        .sheet(item: shared) { (open: MediaOpen) in
            if case let .file(a) = open { FileShareSheet(attachment: a, session: session) }
        }
    }
}

/// An attachment's bytes as a file of its name, for the share sheet and the players.
enum MediaFiles {
    static func write(_ data: Data, for a: AttachmentSummary) throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-media/\(a.attachmentId)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let name = a.name.replacingOccurrences(of: "/", with: "-").isEmpty ? "attachment" : a.name.replacingOccurrences(of: "/", with: "-")
        let url = dir.appendingPathComponent(name)
        try data.write(to: url, options: .atomic)
        return url
    }

    static func remove(_ url: URL?) {
        guard let url else { return }
        try? FileManager.default.removeItem(at: url.deletingLastPathComponent())
    }
}

/// The system's share sheet over a file.
private struct ActivityView: UIViewControllerRepresentable {
    let items: [Any]
    func makeUIViewController(context: Context) -> UIActivityViewController { UIActivityViewController(activityItems: items, applicationActivities: nil) }
    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

/// A file fetched and opened, then handed to the share sheet (Save to
/// Files is among its actions).
private struct FileShareSheet: View {
    let attachment: AttachmentSummary
    let session: LumiaSession
    @State private var file: URL?
    @State private var error = ""

    var body: some View {
        Group {
            if let file {
                ActivityView(items: [file])
            } else {
                VStack(spacing: LumiaTokens.space12) {
                    if error.isEmpty { ProgressView() } else { LumiaErrorText(error) }
                    Text(attachment.name).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .presentationDetents([.medium, .large])
        .task {
            do { file = try MediaFiles.write(try await session.attachmentData(attachment.main), for: attachment) } catch { self.error = error.lumiaMessage }
        }
        .onDisappear { MediaFiles.remove(file) }
    }
}

/// A picture full screen: its display picture (the thumbnail until it
/// comes), pinch to zoom and a double tap, its alt text under it and as
/// what VoiceOver says of it, Save to Photos and Share.
private struct PictureViewer: View {
    @Environment(\.dismiss) private var dismiss
    let attachment: AttachmentSummary
    let session: LumiaSession
    @State private var picture: CGImage?
    @State private var undrawable = false
    @State private var sharing: URL?
    @State private var said = ""

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if let picture {
                ZoomingImage(image: UIImage(cgImage: picture))
                    .ignoresSafeArea()
                    .accessibilityElement()
                    .accessibilityLabel(MediaWords.label(attachment))
                    .accessibilityAddTraits(.isImage)
            } else if undrawable {
                Text("This picture can’t be shown.").font(LumiaTokens.textBody.font).foregroundStyle(.white)
            } else {
                ProgressView().tint(.white)
            }
        }
        .overlay(alignment: .top) {
            HStack(spacing: LumiaTokens.space16) {
                ViewerButton(symbol: "xmark", label: "Close") { dismiss() }
                Spacer()
                ViewerButton(symbol: "square.and.arrow.down", label: "Save to Photos") { Task { await save() } }
                ViewerButton(symbol: "square.and.arrow.up", label: "Share") { Task { await share() } }
            }
            .padding(.horizontal, LumiaTokens.space16)
            .padding(.top, LumiaTokens.space8)
        }
        .overlay(alignment: .bottom) {
            VStack(spacing: LumiaTokens.space6) {
                if !said.isEmpty {
                    Text(said).font(LumiaTokens.textMeta.font).foregroundStyle(.white)
                        .padding(.horizontal, LumiaTokens.space12).padding(.vertical, LumiaTokens.space6)
                        .background(.black.opacity(0.6), in: Capsule())
                }
                if !attachment.alt.isEmpty {
                    Text(attachment.alt).font(LumiaTokens.textBody.font).foregroundStyle(.white)
                        .padding(LumiaTokens.space12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(.black.opacity(0.6))
                        .accessibilityHidden(true)
                }
            }
        }
        .sheet(item: Binding(get: { sharing.map(SharedFile.init) }, set: { if $0 == nil { MediaFiles.remove(sharing); sharing = nil } })) { shared in
            ActivityView(items: [shared.url]).presentationDetents([.medium, .large])
        }
        .task {
            if let thumbnail = attachment.thumbnail { picture = await session.picture(thumbnail, longest: 800).image }
            switch await session.picture(attachment.main) {
            case let .image(image): picture = image
            case .undrawable: if picture == nil { undrawable = true }
            case .failed: break
            }
        }
    }

    private func share() async {
        do { sharing = try MediaFiles.write(try await session.attachmentData(attachment.main), for: attachment) } catch { say(error.lumiaMessage) }
    }

    /// Into the photo library, as the bytes that were sent; asking only to add.
    private func save() async {
        guard await PHPhotoLibrary.requestAuthorization(for: .addOnly) == .authorized else {
            say("Lumia isn’t allowed to add to Photos. Allow it in Settings.")
            return
        }
        do {
            let data = try await session.attachmentData(attachment.main)
            try await PHPhotoLibrary.shared().performChanges {
                PHAssetCreationRequest.forAsset().addResource(with: .photo, data: data, options: nil)
            }
            say("Saved to Photos")
        } catch {
            say("Not saved: \(error.lumiaMessage)")
        }
    }

    private func say(_ words: String) {
        said = words
        UIAccessibility.post(notification: .announcement, argument: words)
        Task {
            try? await Task.sleep(for: .seconds(3))
            if said == words { said = "" }
        }
    }
}

private struct SharedFile: Identifiable {
    let url: URL
    var id: String { url.path }
}

/// A round button over the viewer.
private struct ViewerButton: View {
    let symbol: String
    let label: String
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 16, weight: .semibold)).foregroundStyle(.white)
                .frame(width: 40, height: 40).background(.black.opacity(0.5), in: Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

/// A picture in a scroll view that pinches to zoom up to four times, and
/// a double tap zooms in and back.
private struct ZoomingImage: UIViewRepresentable {
    let image: UIImage

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> UIScrollView {
        let scroll = UIScrollView()
        scroll.delegate = context.coordinator
        scroll.minimumZoomScale = 1
        scroll.maximumZoomScale = 4
        scroll.showsVerticalScrollIndicator = false
        scroll.showsHorizontalScrollIndicator = false
        scroll.backgroundColor = .clear
        let view = UIImageView(image: image)
        view.contentMode = .scaleAspectFit
        view.translatesAutoresizingMaskIntoConstraints = false
        scroll.addSubview(view)
        NSLayoutConstraint.activate([
            view.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor),
            view.heightAnchor.constraint(equalTo: scroll.frameLayoutGuide.heightAnchor),
            view.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor),
            view.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor),
            view.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor),
        ])
        context.coordinator.view = view
        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.doubleTapped(_:)))
        tap.numberOfTapsRequired = 2
        scroll.addGestureRecognizer(tap)
        return scroll
    }

    func updateUIView(_ scroll: UIScrollView, context: Context) {
        if context.coordinator.view?.image !== image { context.coordinator.view?.image = image }
    }

    final class Coordinator: NSObject, UIScrollViewDelegate {
        weak var view: UIImageView?
        func viewForZooming(in scrollView: UIScrollView) -> UIView? { view }
        @objc func doubleTapped(_ tap: UITapGestureRecognizer) {
            guard let scroll = tap.view as? UIScrollView else { return }
            if scroll.zoomScale > 1 {
                scroll.setZoomScale(1, animated: true)
            } else {
                let at = tap.location(in: view)
                let size = CGSize(width: scroll.bounds.width / 2.5, height: scroll.bounds.height / 2.5)
                scroll.zoom(to: CGRect(x: at.x - size.width / 2, y: at.y - size.height / 2, width: size.width, height: size.height), animated: true)
            }
        }
    }
}
