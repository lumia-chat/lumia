// Server settings' Emoji section on a phone (specs/emoji.md 10, the
// "Emoji on phones" board), for whoever holds Manage emoji: how many the
// server has against its limit, Upload emoji (a picture from the photo
// picker or Files, its name made from the file's by the core, changed
// before Save on a screen of its own), and the list oldest first, each row
// its picture, `:name:` and who uploaded it, with a More whose actions are
// a sheet: Rename, in place in the list, and Remove, which asks first. At
// the limit Upload is off and the line over the list says why. What the
// instance refuses is said where it was tried, in the boards' words
// (`EmojiRefusal`).
import LumiaFFI
import LumiaKit
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// A picture from the photo picker as a file, so its name comes with it.
private struct PickedPicture: Transferable {
    let url: URL

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .image) { received in
            let dir = FileManager.default.temporaryDirectory.appendingPathComponent("lumia-emoji/\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let copy = dir.appendingPathComponent(received.file.lastPathComponent)
            try FileManager.default.copyItem(at: received.file, to: copy)
            return PickedPicture(url: copy)
        }
    }
}

/// Where Upload emoji's picture comes from, and what came of it: shared by
/// the section's Upload and the form's "Choose another…".
@MainActor
@Observable
final class EmojiChooser {
    var pickingPhotos = false
    var pickingFiles = false
    var photo: PhotosPickerItem?
    /// The picture chosen, which opens the form.
    var chosen: EmojiUploadFile?
    var error = ""

    /// A picture from the photo picker: the file it hands over, read off
    /// the main actor and refused over 8 MB before it is read whole or
    /// drawn (`EmojiUploadFile.read`); or, where it hands over no file, its
    /// data, refused over 8 MB before it is drawn.
    func take(photo item: PhotosPickerItem) async {
        error = ""
        do {
            if let picked = try await item.loadTransferable(type: PickedPicture.self) {
                defer { try? FileManager.default.removeItem(at: picked.url.deletingLastPathComponent()) }
                took(await EmojiUploadFile.read(picked.url))
            } else if let data = try await item.loadTransferable(type: Data.self) {
                let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "png"
                took(await EmojiUploadFile.make(data: data, fileName: "emoji.\(ext)"))
            } else {
                error = EmojiWords.unreadable
            }
        } catch {
            self.error = EmojiWords.unreadable
        }
    }

    /// A picture from Files, read the same way.
    func take(file url: URL) async {
        error = ""
        took(await EmojiUploadFile.read(url))
    }

    private func took(_ read: EmojiUploadRead) {
        switch read {
        case let .chosen(file): chosen = file
        case let .refused(words): error = words
        }
    }
}

extension View {
    /// The photo picker and Files, as Upload emoji's menu opens them.
    func emojiChooser(_ chooser: EmojiChooser) -> some View {
        @Bindable var chooser = chooser
        return self
            .photosPicker(isPresented: $chooser.pickingPhotos, selection: $chooser.photo, matching: .images, preferredItemEncoding: .compatible)
            .onChange(of: chooser.photo) { _, item in
                guard let item else { return }
                chooser.photo = nil
                Task { await chooser.take(photo: item) }
            }
            .fileImporter(isPresented: $chooser.pickingFiles, allowedContentTypes: [.image]) { result in
                if case let .success(url) = result { Task { await chooser.take(file: url) } }
            }
    }
}

/// Upload emoji's way to a picture: the photo picker or Files.
private struct ChooseMenu<Label: View>: View {
    let chooser: EmojiChooser
    @ViewBuilder let label: () -> Label

    var body: some View {
        Menu {
            Button(EmojiWords.photos, systemImage: "photo.on.rectangle") { chooser.pickingPhotos = true }
            Button(EmojiWords.files, systemImage: "folder") { chooser.pickingFiles = true }
        } label: {
            label()
        }
    }
}

/// What a row's More opened, or the question its Remove asks.
private enum EmojiSheet: Identifiable {
    case row(EmojiSummary)
    case remove(EmojiSummary)

    var id: String {
        switch self {
        case let .row(e): "row-\(e.emojiId)"
        case let .remove(e): "remove-\(e.emojiId)"
        }
    }
}

/// A row being renamed in place: what its field holds, and why it was refused.
private struct Renaming: Equatable {
    let emojiID: String
    var value: String
    var error = ""
    var busy = false
}

/// The Emoji section: the "Emoji on phones" board's list.
struct ServerEmojiScreen: View {
    let session: LumiaSession
    let serverID: String
    @State private var chooser = EmojiChooser()
    @State private var sheet: EmojiSheet?
    @State private var renaming: Renaming?
    @State private var removing = false
    @State private var removeError = ""
    @FocusState private var renameFocused: Bool

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }

    var body: some View {
        let emoji = server?.emoji ?? []
        let limit = server?.emojiLimit ?? 0
        PanelScreen(back: settingsBack(server), title: rolesSectionLabel(section: .emoji), subtitle: server.map { "\($0.name) · \(EmojiWords.count($0.emoji.count, limit: $0.emojiLimit))" }) {
            ScrollViewReader { proxy in
            ScrollView {
                if let server {
                    VStack(alignment: .leading, spacing: LumiaTokens.space12) {
                        if !server.canManageEmoji {
                            // What let this account in was taken away while the screen was open.
                            Text(EmojiWords.notAllowed(server: server.name)).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkMuted.color)
                        } else {
                            Text(EmojiWords.lead(server: server.name, count: emoji.count, limit: limit)).font(RoleMetrics.lead.font)
                                .foregroundStyle(LumiaTokens.inkMuted.color)
                                .fixedSize(horizontal: false, vertical: true)
                                .accessibilityIdentifier("emoji-lead")
                            ChooseMenu(chooser: chooser) {
                                Label(EmojiWords.upload, systemImage: "plus").frame(maxWidth: .infinity)
                            }
                            .buttonStyle(.lumiaSecondary)
                            .disabled(EmojiWords.atLimit(count: emoji.count, limit: limit))
                            .accessibilityIdentifier("emoji-upload")
                            if !chooser.error.isEmpty, chooser.chosen == nil { LumiaErrorText(chooser.error) }
                            VStack(spacing: LumiaTokens.space6) {
                                ForEach(emoji, id: \.emojiId) { e in row(e, server: server).id(e.emojiId) }
                            }
                            .accessibilityElement(children: .contain)
                            .accessibilityLabel("Emoji")
                        }
                    }
                    .padding(LumiaTokens.space16)
                }
            }
            // A row renamed in place is brought into view over the keyboard.
            .onChange(of: renaming?.emojiID) { _, id in
                if let id { withAnimation { proxy.scrollTo(id, anchor: .center) } }
            }
            }
        }
        .emojiChooser(chooser)
        .navigationDestination(item: Bindable(chooser).chosen) { file in
            EmojiUploadScreen(session: session, serverID: serverID, chooser: chooser, file: file)
        }
        .sheet(item: $sheet) { sheet in
            switch sheet {
            case let .row(e):
                EmojiRowSheet(session: session, emoji: e, rename: {
                    self.sheet = nil
                    startRenaming(e)
                }, remove: {
                    removeError = ""
                    self.sheet = .remove(e)
                })
            case let .remove(e):
                AskSheet(title: EmojiWords.removeTitle(e.name), lines: [EmojiWords.removeLine], confirm: EmojiWords.remove, danger: true, cancel: EmojiWords.keep, busy: removing, error: removeError, go: { Task { await remove(e) } }, stop: { self.sheet = nil })
            }
        }
        #if DEBUG
        .task { devOpen() }
        #endif
    }

    private func row(_ e: EmojiSummary, server: ServerSummary) -> some View {
        let renamingThis = renaming?.emojiID == e.emojiId
        return VStack(alignment: .leading, spacing: LumiaTokens.space8) {
            HStack(spacing: LumiaTokens.space12) {
                picture(e)
                if renamingThis {
                    renameField(e)
                } else {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(verbatim: ":\(e.name):").font(LumiaTokens.textButtonSecondary.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                        Text(EmojiWords.uploaded(by: session.name(of: e.createdByAccountId), animated: e.animated))
                            .font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color).lineLimit(2)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .accessibilityElement(children: .combine)
                    Spacer(minLength: 0)
                    Button { sheet = .row(e) } label: {
                        Image(systemName: "ellipsis").font(.system(size: 17, weight: .semibold)).foregroundStyle(LumiaTokens.iconStrong.color)
                            .frame(width: RoleMetrics.more, height: RoleMetrics.more)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("More for :\(e.name):")
                    .accessibilityIdentifier("emoji-more-\(e.name)")
                }
            }
            if renamingThis, let renaming {
                if !renaming.error.isEmpty { LumiaErrorText(renaming.error).accessibilityIdentifier("emoji-rename-error") }
                HStack(spacing: LumiaTokens.space10) {
                    Spacer(minLength: 0)
                    Button("Cancel") { self.renaming = nil }.buttonStyle(.lumiaSecondary).disabled(renaming.busy)
                    Button("Save") { Task { await rename(e) } }.buttonStyle(.lumiaPrimary)
                        .disabled(renaming.busy || renaming.value.trimmingCharacters(in: .whitespaces).isEmpty)
                        .accessibilityIdentifier("emoji-rename-save")
                }
            }
        }
        .padding(.vertical, LumiaTokens.space8)
        .padding(.horizontal, LumiaTokens.space12)
        .frame(maxWidth: .infinity, minHeight: RoleMetrics.roleRow, alignment: .leading)
        .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.lineStrong.color))
    }

    private func picture(_ e: EmojiSummary) -> some View {
        Group {
            if let drawing = session.drawing(of: e) { LumiaCustomEmoji(drawing, side: EmojiMetrics.row) } else { Color.clear }
        }
        .frame(width: EmojiMetrics.row, height: EmojiMetrics.row)
        .accessibilityHidden(true)
    }

    /// The name between its colons, as the boards draw the field.
    private func renameField(_ e: EmojiSummary) -> some View {
        EmojiNameField(name: Binding(get: { renaming?.value ?? "" }, set: { renaming?.value = $0 }), label: "New name for :\(e.name):", refused: !(renaming?.error.isEmpty ?? true), focused: $renameFocused)
            .disabled(renaming?.busy == true)
            .onSubmit { Task { await rename(e) } }
    }

    private func startRenaming(_ e: EmojiSummary) {
        renaming = Renaming(emojiID: e.emojiId, value: e.name)
        renameFocused = true
    }

    private func rename(_ e: EmojiSummary) async {
        guard var now = renaming, !now.busy else { return }
        let name = now.value
        if name.trimmingCharacters(in: .whitespaces) == e.name { renaming = nil; return }
        if let refused = EmojiRefusal.before(name: name, bytes: 0) {
            now.error = refused.words
            renaming = now
            return
        }
        now.busy = true
        now.error = ""
        renaming = now
        do {
            try await session.renameEmoji(serverID: serverID, emojiID: e.emojiId, name: name)
            renaming = nil
            AccessibilityNotification.Announcement("Renamed :\(EmojiWords.sent(name)):").post()
        } catch {
            renaming?.busy = false
            renaming?.error = EmojiRefusal.words(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage, server: server?.name ?? "This server", name: EmojiWords.sent(name)).words
        }
    }

    private func remove(_ e: EmojiSummary) async {
        removing = true
        removeError = ""
        defer { removing = false }
        do {
            try await session.removeEmoji(serverID: serverID, emojiID: e.emojiId)
            sheet = nil
            AccessibilityNotification.Announcement("Removed :\(e.name):").post()
        } catch {
            removeError = EmojiRefusal.words(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage, server: server?.name ?? "This server", name: e.name).words
        }
    }

    #if DEBUG
    /// A smoke run's way in (`LUMIA_DEV_SCREEN=emoji`): `LUMIA_DEV_EMOJI_FILE`
    /// opens the upload form on that file; `LUMIA_DEV_EMOJI` names a row,
    /// whose sheet opens, or with `LUMIA_DEV_ASK` `remove` its question, or
    /// `rename` its field in place, holding `LUMIA_DEV_EMOJI_NAME` when given.
    private func devOpen() {
        let env = ProcessInfo.processInfo.environment
        if let path = env["LUMIA_DEV_EMOJI_FILE"] {
            Task { if case let .chosen(file) = await EmojiUploadFile.read(URL(fileURLWithPath: path)) { chooser.chosen = file } }
            return
        }
        guard let name = env["LUMIA_DEV_EMOJI"], let e = server?.emoji.first(where: { $0.name == name }) else { return }
        switch env["LUMIA_DEV_ASK"] {
        case "remove": sheet = .remove(e)
        case "rename":
            renaming = Renaming(emojiID: e.emojiId, value: env["LUMIA_DEV_EMOJI_NAME"] ?? e.name)
            if env["LUMIA_DEV_EMOJI_NAME"] != nil { Task { await rename(e) } }
        default: sheet = .row(e)
        }
    }
    #endif
}

/// A name between its colons (the boards' `TextField` with `before` and
/// `after`), at most 32 characters, in the danger's line when refused.
struct EmojiNameField: View {
    @Binding var name: String
    let label: String
    var refused = false
    var focused: FocusState<Bool>.Binding?

    var body: some View {
        HStack(spacing: LumiaTokens.space2) {
            Text(verbatim: ":").foregroundStyle(LumiaTokens.inkQuiet.color).accessibilityHidden(true)
            Group {
                if let focused { field.focused(focused) } else { field }
            }
            Text(verbatim: ":").foregroundStyle(LumiaTokens.inkQuiet.color).accessibilityHidden(true)
        }
        .font(LumiaTokens.textBody.font)
        .padding(.horizontal, LumiaTokens.space10)
        .frame(height: LumiaTokens.controlHeight)
        .background(LumiaTokens.field.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius8))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius8).stroke(refused ? LumiaTokens.danger.color : LumiaTokens.borderForm.color))
    }

    private var field: some View {
        TextField("", text: $name)
            .textFieldStyle(.plain)
            .foregroundStyle(LumiaTokens.inkField.color)
            .textInputAutocapitalization(.never).autocorrectionDisabled()
            .submitLabel(.done)
            .onChange(of: name) { _, now in if now.count > 32 { name = String(now.prefix(32)) } }
            .accessibilityLabel(label)
            .accessibilityIdentifier("emoji-name")
    }
}

/// How tall a sheet's words stand, so the sheet is no taller than they are.
private struct RowSheetHeight: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

/// A row's actions, in a sheet (the board's): the picture, `:name:`, who
/// uploaded it, then Rename and Remove. No Cancel: the sheet is let go by
/// a swipe or a tap outside.
private struct EmojiRowSheet: View {
    let session: LumiaSession
    let emoji: EmojiSummary
    let rename: () -> Void
    let remove: () -> Void
    @State private var height: CGFloat = 0

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            HStack(spacing: LumiaTokens.space14) {
                Group {
                    if let drawing = session.drawing(of: emoji) { LumiaCustomEmoji(drawing, side: EmojiMetrics.sheet) } else { Color.clear }
                }
                .frame(width: EmojiMetrics.sheet, height: EmojiMetrics.sheet)
                .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    Text(verbatim: ":\(emoji.name):").font(RoleMetrics.sheetTitle.font).foregroundStyle(LumiaTokens.ink.color)
                        .lineLimit(1).minimumScaleFactor(0.6)
                        .accessibilityAddTraits(.isHeader)
                    Text(EmojiWords.uploaded(by: session.name(of: emoji.createdByAccountId), animated: emoji.animated))
                        .font(LumiaTokens.textSubtitle.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                }
            }
            VStack(spacing: LumiaTokens.space10) {
                Button(action: rename) { Text(EmojiWords.rename).frame(maxWidth: .infinity) }
                    .buttonStyle(.lumiaSecondary)
                    .accessibilityIdentifier("emoji-rename")
                Button(action: remove) { Text(EmojiWords.remove).frame(maxWidth: .infinity) }
                    .buttonStyle(KickButtonStyle())
                    .accessibilityIdentifier("emoji-remove")
            }
        }
        .padding(.horizontal, LumiaTokens.space24)
        .padding(.top, LumiaTokens.space28)
        .padding(.bottom, LumiaTokens.space16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GeometryReader { g in Color.clear.preference(key: RowSheetHeight.self, value: g.size.height) })
        .onPreferenceChange(RowSheetHeight.self) { height = $0 }
        .presentationDetents(height > 0 ? [.height(height)] : [.medium])
        .presentationDragIndicator(.visible)
        .presentationBackground(LumiaTokens.surface.color)
    }
}

/// Upload emoji, once a picture is chosen (the board's `EmojiUpload` on a
/// screen of its own): the picture as chosen with its file's name and size
/// and "Choose another…", its name made from the file's name by the core to
/// change before Save, and Cancel and Save. A refusal is said under what
/// was refused.
struct EmojiUploadScreen: View {
    @Environment(\.dismiss) private var dismiss
    let session: LumiaSession
    let serverID: String
    let chooser: EmojiChooser
    let file: EmojiUploadFile
    @State private var name = ""
    @State private var nameError = ""
    @State private var pictureError = ""
    @State private var formError = ""
    @State private var busy = false

    private var server: ServerSummary? { session.snapshot?.servers.first { $0.serverId == serverID } }

    var body: some View {
        PanelScreen(back: rolesSectionLabel(section: .emoji), title: EmojiWords.upload, subtitle: server.map { "\($0.name) · \(EmojiWords.count($0.emoji.count, limit: $0.emojiLimit))" }) {
            ScrollView {
                VStack(alignment: .leading, spacing: LumiaTokens.space16) {
                    HStack(spacing: LumiaTokens.space14) {
                        Group {
                            if let preview = file.preview {
                                LumiaCustomEmoji(EmojiDrawing(emojiID: "", name: name, width: CGFloat(preview.frames[0].width), height: CGFloat(preview.frames[0].height), animated: preview.animated, picture: preview), side: EmojiMetrics.upload)
                            } else {
                                Image(systemName: "photo").font(.system(size: LumiaTokens.space24)).foregroundStyle(LumiaTokens.inkQuiet.color)
                            }
                        }
                        .frame(width: EmojiMetrics.upload + LumiaTokens.space16, height: EmojiMetrics.upload + LumiaTokens.space16)
                        .background(LumiaTokens.inset.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).strokeBorder(pictureError.isEmpty ? LumiaTokens.line.color : LumiaTokens.danger.color))
                        VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                            Text(file.fileName).font(LumiaTokens.textButtonSecondary.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1).truncationMode(.middle)
                            Text(file.size).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
                            ChooseMenu(chooser: chooser) { Text(EmojiWords.chooseAnother) }
                                .buttonStyle(.lumiaLink)
                                .disabled(busy)
                        }
                        Spacer(minLength: 0)
                    }
                    if !pictureError.isEmpty { LumiaErrorText(pictureError).accessibilityIdentifier("emoji-picture-error") }
                    // "Choose another…" refused what it was given; the picture before stays.
                    if !chooser.error.isEmpty { LumiaErrorText(chooser.error) }
                    VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                        Text("Name").font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                        EmojiNameField(name: $name, label: "Name", refused: !nameError.isEmpty)
                            .disabled(busy)
                            .onSubmit { Task { await save() } }
                        if nameError.isEmpty {
                            RoleHint(EmojiWords.nameHint(EmojiWords.sent(name)))
                        } else {
                            LumiaErrorText(nameError).accessibilityIdentifier("emoji-name-error")
                        }
                    }
                    if !formError.isEmpty { LumiaErrorText(formError).accessibilityIdentifier("emoji-form-error") }
                    HStack(spacing: LumiaTokens.space12) {
                        Spacer(minLength: 0)
                        Button("Cancel") { chooser.chosen = nil }.buttonStyle(.lumiaSecondary).disabled(busy)
                        Button("Save") { Task { await save() } }.buttonStyle(.lumiaPrimary)
                            .disabled(busy || name.trimmingCharacters(in: .whitespaces).isEmpty)
                            .accessibilityIdentifier("emoji-save")
                    }
                }
                .padding(LumiaTokens.space16)
            }
        }
        .onAppear { if name.isEmpty { name = emojiNameFromFile(fileName: file.fileName) } }
        // "Choose another…": the new picture, and a name made from its file.
        .onChange(of: file.id) { _, _ in
            name = emojiNameFromFile(fileName: file.fileName)
            pictureError = ""
            formError = ""
        }
        .onChange(of: name) { _, _ in nameError = "" }
    }

    private func save() async {
        guard !busy else { return }
        nameError = ""
        pictureError = ""
        formError = ""
        if let refused = EmojiRefusal.before(name: name, bytes: file.data.count) {
            say(refused)
            return
        }
        busy = true
        defer { busy = false }
        do {
            try await session.uploadEmoji(serverID: serverID, name: name, picture: file.data, mediaType: file.mediaType)
            AccessibilityNotification.Announcement("Uploaded :\(EmojiWords.sent(name)):").post()
            chooser.chosen = nil
        } catch {
            say(EmojiRefusal.words(code: error.lumiaCode, details: error.lumiaDetails, message: error.lumiaMessage, server: server?.name ?? "This server", name: EmojiWords.sent(name)))
        }
    }

    private func say(_ refused: (place: EmojiRefusalPlace, words: String)) {
        switch refused.place {
        case .name: nameError = refused.words
        case .picture: pictureError = refused.words
        case .form: formError = refused.words
        }
    }
}
