// Attachments and link cards on the phone (specs/media.md 5 and 6): the
// composer's attach button (the system's photo picker for pictures and
// videos, the document picker for any file), the strip over the field
// with each file's picture, name, size or state and its ×, alt text for a
// picture, and the cards its links will have; a message's attachments
// under its words (pictures at their shape or in a grid, a GIF playing
// unless Reduce Motion is on, a video's poster, an audio player, a file
// row) and its cards; the full-screen picture with pinch zoom, Save to
// Photos and Share; a video played full screen; a file shared or saved
// through the share sheet. Images and Embeds in Preferences draw every
// attachment as a file row and no cards when set to links. The kit has
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
                strip.failed(id, MediaWords.refusal(code: error.lumiaCode, message: error.lumiaMessage))
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
        for url in previews.update(links: cardLinks(body: draft)) {
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

/// What a message carries, opened: a picture in the viewer, a video in the
/// player, anything else through the share sheet.
enum MediaOpen: Identifiable {
    case picture(AttachmentSummary)
    case video(AttachmentSummary)
    case file(AttachmentSummary)

    var id: String {
        switch self {
        case let .picture(a): "picture-\(a.attachmentId)"
        case let .video(a): "video-\(a.attachmentId)"
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
        case .video: .video(a)
        case .audio, .file: .file(a)
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
/// as a row and no cards.
struct MessageMedia: View {
    @Environment(Preferences.self) private var preferences
    let attachments: [AttachmentSummary]
    let cards: [CardSummary]
    let session: LumiaSession
    let open: (MediaOpen) -> Void
    var editing: MediaEditing?

    private static let gap = LumiaTokens.space2

    private var shown: [AttachmentSummary] { attachments.filter { !(editing?.removed.removedAttachments.contains($0.attachmentId) ?? false) } }
    private var shownCards: [CardSummary] {
        guard preferences.embeds == .shown || editing != nil else { return [] }
        return cards.filter { !(editing?.removed.removedCards.contains($0.url) ?? false) }
    }

    var body: some View {
        let asRows = preferences.images == .link
        let visual = asRows ? [] : shown.filter { AttachmentKind($0.kind).visual }
        let rows = asRows ? shown : shown.filter { !AttachmentKind($0.kind).visual }
        VStack(alignment: .leading, spacing: LumiaTokens.space6) {
            if !visual.isEmpty {
                PictureGrid(sizes: visual.map { ($0.width, $0.height) }, gap: Self.gap) {
                    ForEach(visual, id: \.attachmentId) { a in
                        removable(a.attachmentId) {
                            AttachmentTile(attachment: a, session: session, single: visual.count == 1) { open(.of(a)) }
                        }
                    }
                }
            }
            ForEach(rows, id: \.attachmentId) { a in
                removable(a.attachmentId) {
                    if AttachmentKind(a.kind) == .audio, !asRows {
                        AudioRow(attachment: a, session: session) { open(.file(a)) }
                    } else {
                        FileRow(attachment: a, asFile: asRows) { open(.of(a)) }
                    }
                }
            }
            ForEach(shownCards, id: \.url) { card in
                if let editing {
                    HStack(alignment: .top, spacing: LumiaTokens.space8) {
                        LinkCardView(card: card, session: session)
                        RemoveButton(label: "Remove the card for \(card.host)") { editing.removeCard(card.url) }
                    }
                } else {
                    LinkCardView(card: card, session: session)
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
/// Motion is on, when a tap plays it first. A picture whose header is
/// over the bounds is drawn as what it is, a file.
private struct AttachmentTile: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let attachment: AttachmentSummary
    let session: LumiaSession
    let single: Bool
    let open: () -> Void
    @State private var picture: CGImage?
    @State private var placeholder: CGImage?
    @State private var undrawable = false
    @State private var animation: UIImage?
    @State private var playing = false

    private var kind: AttachmentKind { AttachmentKind(attachment.kind) }

    var body: some View {
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
                Image(systemName: "play.fill")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 44, height: 44)
                    .background(.black.opacity(0.55), in: Circle())
            }
        }
        .overlay(alignment: .bottomLeading) {
            if kind == .gif, reduceMotion, !playing { MediaBadge(text: MediaWords.gifBadge) }
            if kind == .video, attachment.durationMs > 0 { MediaBadge(text: MediaWords.duration(ms: attachment.durationMs)) }
        }
        .clipShape(RoundedRectangle(cornerRadius: single ? LumiaTokens.radius12 : LumiaTokens.radius6))
        .overlay(RoundedRectangle(cornerRadius: single ? LumiaTokens.radius12 : LumiaTokens.radius6).stroke(LumiaTokens.line.color))
        .contentShape(Rectangle())
        .onTapGesture {
            // Reduce Motion: the first tap plays a still GIF in place.
            if kind == .gif, reduceMotion, !playing, animation != nil { playing = true } else { open() }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(MediaWords.label(attachment, asFile: undrawable))
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("attachment")
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
    let open: () -> Void

    private var kind: AttachmentKind { AttachmentKind(attachment.kind) }
    private var pdf: Bool { MediaWords.symbol(kind: kind, mediaType: attachment.mediaType, name: attachment.name) == "doc.richtext" }

    var body: some View {
        Button(action: open) {
            HStack(spacing: LumiaTokens.space10) {
                Image(systemName: MediaWords.symbol(kind: kind, mediaType: attachment.mediaType, name: attachment.name))
                    .font(.system(size: 18))
                    .foregroundStyle(pdf ? LumiaTokens.filePdfInk.color : LumiaTokens.inkSecondary.color)
                    .frame(width: 40, height: 40)
                    .background(pdf ? LumiaTokens.filePdf.color : LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    Text(attachment.name).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1).truncationMode(.middle)
                    Text(MediaWords.detail(kind: kind, sizeBytes: attachment.sizeBytes, durationMs: attachment.durationMs))
                        .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
                }
                Spacer(minLength: 0)
                Image(systemName: kind.visual ? "arrow.up.left.and.arrow.down.right" : "square.and.arrow.down")
                    .font(.system(size: 15))
                    .foregroundStyle(LumiaTokens.inkSecondary.color)
                    .frame(width: 32, height: 32)
            }
            .padding(LumiaTokens.space8)
            .frame(maxWidth: 360, alignment: .leading)
            .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(MediaWords.label(attachment, asFile: asFile || kind == .file))
        .accessibilityHint(kind.visual ? "Opens it" : "Opens the share sheet to save or share it")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("attachment")
    }
}

/// An audio file: play and pause in place, its name and length, and Save.
private struct AudioRow: View {
    let attachment: AttachmentSummary
    let session: LumiaSession
    let share: () -> Void
    @State private var player: AVAudioPlayer?
    @State private var playing = false
    @State private var loading = false
    @State private var error = ""

    var body: some View {
        HStack(spacing: LumiaTokens.space10) {
            Button(action: toggle) {
                Group {
                    if loading { ProgressView() } else { Image(systemName: playing ? "pause.fill" : "play.fill").font(.system(size: 16)) }
                }
                .foregroundStyle(LumiaTokens.onAccent.color)
                .frame(width: 40, height: 40)
                .background(LumiaTokens.accent.color, in: Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(playing ? "Pause" : "Play")
            VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                Text(attachment.name).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1).truncationMode(.middle)
                Text(error.isEmpty ? MediaWords.detail(kind: .audio, sizeBytes: attachment.sizeBytes, durationMs: attachment.durationMs) : error)
                    .font(LumiaTokens.textMeta.font).foregroundStyle(error.isEmpty ? LumiaTokens.inkMuted.color : LumiaTokens.danger.color)
            }
            .accessibilityElement(children: .combine)
            Spacer(minLength: 0)
            Button(action: share) {
                Image(systemName: "square.and.arrow.down").font(.system(size: 15)).foregroundStyle(LumiaTokens.inkSecondary.color).frame(width: 32, height: 32)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Save \(attachment.name)")
        }
        .padding(LumiaTokens.space8)
        .frame(maxWidth: 360, alignment: .leading)
        .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
        .onDisappear { player?.stop(); playing = false }
    }

    private func toggle() {
        if let player {
            if player.isPlaying { player.pause(); playing = false } else { player.play(); playing = true }
            return
        }
        loading = true
        Task {
            defer { loading = false }
            do {
                let data = try await session.attachmentData(attachment.main)
                try? AVAudioSession.sharedInstance().setCategory(.playback)
                let made = try AVAudioPlayer(data: data)
                player = made
                made.play()
                playing = true
                // Back to Play once it has played through.
                while made.isPlaying { try? await Task.sleep(for: .milliseconds(250)) }
                playing = false
            } catch {
                self.error = "This can’t be played here."
            }
        }
    }
}

/// A link's card: the site's host as the link names it (never what the
/// page says it is), the title, the description, the picture with a play
/// symbol for a video page; tapped, the link opens in the browser.
struct LinkCardView: View {
    @Environment(\.openURL) private var openURL
    let card: CardSummary
    let session: LumiaSession
    var compact = false
    @State private var picture: CGImage?

    private var bar: Color {
        guard card.color.count == 7, card.color.hasPrefix("#"), let rgb = UInt32(card.color.dropFirst(), radix: 16) else { return LumiaTokens.lineStrong.color }
        return Color(.sRGB, red: Double(rgb >> 16 & 0xFF) / 255, green: Double(rgb >> 8 & 0xFF) / 255, blue: Double(rgb & 0xFF) / 255)
    }

    private var hasPicture: Bool { !card.imageUrl.isEmpty || card.image != nil }
    /// A wide picture (a page's banner) is drawn whole under the words; a
    /// squarer one, or any in the composer's preview, small at their side.
    private var wide: Bool { !compact && card.imageHeight > 0 && Double(card.imageWidth) / Double(card.imageHeight) >= 1.4 }

    var body: some View {
        Button {
            if let url = URL(string: card.url) { openURL(url) }
        } label: {
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
                        cardPicture.frame(width: compact ? 44 : 72, height: compact ? 44 : 72)
                    }
                }
                if hasPicture, wide {
                    cardPicture.aspectRatio(CGFloat(card.imageWidth) / CGFloat(card.imageHeight), contentMode: .fit)
                        .frame(maxHeight: 200, alignment: .leading)
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
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(MediaWords.cardLabel(card))
        .accessibilityValue(card.description)
        .accessibilityHint("Opens the link in the browser")
        .accessibilityAddTraits(.isLink)
        .accessibilityIdentifier("card")
        .task(id: card.url) {
            if hasPicture { picture = await session.cardPicture(card) }
        }
    }

    /// The page's picture, with a play symbol over a video page's.
    private var cardPicture: some View {
        ZStack {
            LumiaTokens.inset.color
            if let picture { Image(decorative: picture, scale: 1).resizable().scaledToFill() }
            if card.video {
                Image(systemName: "play.fill").font(.system(size: 12, weight: .semibold)).foregroundStyle(.white)
                    .frame(width: 28, height: 28).background(.black.opacity(0.55), in: Circle())
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius8))
    }
}

// MARK: Opening

extension View {
    /// What a conversation's attachments open into: the picture viewer and
    /// the video player full screen, the share sheet for anything else.
    func mediaOpening(_ opening: Binding<MediaOpen?>, session: LumiaSession) -> some View {
        let full = Binding<MediaOpen?>(get: { opening.wrappedValue?.fullScreen == true ? opening.wrappedValue : nil }, set: { if $0 == nil { opening.wrappedValue = nil } })
        let shared = Binding<MediaOpen?>(get: { opening.wrappedValue?.fullScreen == false ? opening.wrappedValue : nil }, set: { if $0 == nil { opening.wrappedValue = nil } })
        return fullScreenCover(item: full) { (open: MediaOpen) in
            switch open {
            case let .picture(a): PictureViewer(attachment: a, session: session)
            case let .video(a): VideoViewer(attachment: a, session: session)
            case .file: EmptyView()
            }
        }
        .sheet(item: shared) { (open: MediaOpen) in
            if case let .file(a) = open { FileShareSheet(attachment: a, session: session) }
        }
    }
}

/// An attachment's bytes as a file of its name, for the share sheet and the player.
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

/// A video full screen: fetched and opened into a file of its name, then
/// the system's player. It never plays by itself before it is opened.
private struct VideoViewer: View {
    @Environment(\.dismiss) private var dismiss
    let attachment: AttachmentSummary
    let session: LumiaSession
    @State private var player: AVPlayer?
    @State private var file: URL?
    @State private var error = ""

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if let player {
                VideoPlayer(player: player).ignoresSafeArea()
                    .accessibilityLabel(MediaWords.label(attachment))
            } else if !error.isEmpty {
                LumiaErrorText(error)
            } else {
                ProgressView().tint(.white)
            }
        }
        .overlay(alignment: .topLeading) {
            ViewerButton(symbol: "xmark", label: "Close") { dismiss() }
                .padding(.horizontal, LumiaTokens.space16)
                .padding(.top, LumiaTokens.space8)
        }
        .task {
            do {
                let url = try MediaFiles.write(try await session.attachmentData(attachment.main), for: attachment)
                file = url
                try? AVAudioSession.sharedInstance().setCategory(.playback)
                let made = AVPlayer(url: url)
                player = made
                made.play()
            } catch {
                self.error = error.lumiaMessage
            }
        }
        .onDisappear {
            player?.pause()
            MediaFiles.remove(file)
        }
    }
}
