// The server rail, after the phone boards and the "Server folders" boards
// (architecture 6.9): Home first, then the account's servers and folders
// in its order, then +, and the account's own avatar at its foot. Every
// tile is a rounded square but that avatar. A closed folder is one tile
// in its tone with its first four servers small and its servers' mentions
// added up; open, it is its folder glyph and its servers in a column of
// its tone; a tap opens or closes it, and which are open is this phone's.
// A long press lifts a tile: moving drags it (between tiles an insertion
// line in the accent, onto a server's middle a folder of the two, onto a
// folder into it, out of an open folder out of it), letting go without
// moving opens its menu as the system's sheet. VoiceOver finds the same
// menu as each tile's actions, and every move is said.
import LumiaFFI
import LumiaKit
import SwiftUI
import UIKit

struct PhoneRail: View {
    @Environment(AppStore.self) private var store
    @Environment(\.colorScheme) private var scheme
    let session: LumiaSession
    @Binding var place: Place
    let add: () -> Void
    /// Opens one of the account's own screens, from the menu on the avatar.
    let open: (Screen) -> Void

    /// The folders open on this phone (not the account's).
    @AppStorage(OpenFolders.key) private var openStored = ""
    /// Where each tile is drawn, in the rail's space: a server's or a
    /// folder's tile by its id, an open folder's column as `group:<id>`.
    @State private var frames: [String: CGRect] = [:]
    /// The tile a long press lifted, where it is while it moves, and from where it was taken.
    @State private var lifted: String?
    @State private var dragged: CGPoint?
    @State private var grab: CGSize = .zero
    @State private var target: RailDropTarget?
    @GestureState private var pressing = false
    /// The tile whose menu is open, the server whose folders are listed, the folder being set.
    @State private var menu: String?
    @State private var moveTo: String?
    @State private var settings: FolderSettingsTarget?

    private static let space = "rail"
    private var rail: [RailItem] { session.rail }
    private var openFolders: OpenFolders { OpenFolders(stored: openStored) }

    private func server(_ id: String) -> ServerSummary? { session.snapshot?.servers.first { $0.serverId == id } }

    var body: some View {
        VStack(spacing: LumiaTokens.space12) {
            // Home's count: the requests that wait and the personal conversations' unread messages.
            Button { place = .home } label: {
                square(current: place == .home) {
                    Image("Logo").resizable().scaledToFit().padding(LumiaTokens.space6)
                }
                .overlay(alignment: .topTrailing) { badge((session.snapshot?.pendingRequests.count ?? 0) + Int(session.personalMentions), unread: false, out: LumiaTokens.space10 / 2) }
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Home")
            .accessibilityAddTraits(place == .home ? .isSelected : [])
            Rectangle().fill(LumiaTokens.railDivider.color).frame(width: LumiaTokens.space24, height: 1)
            ScrollView(.vertical) {
                VStack(spacing: LumiaTokens.space12) {
                    ForEach(rail, id: \.id) { item in
                        switch item {
                        case let .server(serverId): serverTile(serverId)
                        case let .folder(folderId, _, tone, serverIds):
                            if openFolders.isOpen(folderId) {
                                openFolder(folderId, tone: AvatarTone.folder(tone, id: folderId), servers: serverIds)
                            } else {
                                closedFolder(folderId, tone: AvatarTone.folder(tone, id: folderId), servers: serverIds)
                            }
                        }
                    }
                    Button(action: add) {
                        Image(systemName: "plus").font(.system(size: LumiaTokens.space18, weight: .medium)).foregroundStyle(LumiaTokens.inkQuiet.color)
                            .frame(width: LumiaTokens.tileSize, height: LumiaTokens.tileSize)
                            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.borderDashed.color, style: dashed))
                            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius14))
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Add a server")
                }
                // Room for the rings and counts that stand outside the tiles.
                .padding(.vertical, LumiaTokens.space6)
                .frame(maxWidth: .infinity)
            }
            .scrollIndicators(.hidden)
            .scrollBounceBehavior(.basedOnSize)
            accountMenu
        }
        .padding(.top, LumiaTokens.space8)
        .padding(.bottom, LumiaTokens.space12)
        .frame(width: LumiaTokens.railWidth)
        .frame(maxHeight: .infinity)
        .background(LumiaTokens.rail.color.ignoresSafeArea())
        .coordinateSpace(name: Self.space)
        .onPreferenceChange(RailFrames.self) { frames = $0 }
        .overlay { dragOverlay }
        .onChange(of: pressing) { _, now in
            // A press the system took away (a call, a sheet) puts the tile back.
            if !now { DispatchQueue.main.async { lifted = nil; dragged = nil; target = nil } }
        }
        .onChange(of: rail.filter(\.isFolder).map(\.id)) { _, _ in
            let kept = openFolders.kept(in: rail)
            if kept != openFolders { openStored = kept.stored }
        }
        .confirmationDialog(menu.map(session.railName) ?? "", isPresented: present($menu), titleVisibility: .visible, presenting: menu) { id in
            ForEach(RailMenu.actions(for: id, in: rail), id: \.self) { action in
                Button(action == .moveToFolder ? "Move to folder…" : action.title, role: action == .deleteFolder ? .destructive : nil) {
                    if action == .moveToFolder {
                        // The list of folders is a sheet of its own, once this one has gone.
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { moveTo = id }
                    } else {
                        run(action, on: id)
                    }
                }
            }
        }
        .background {
            Color.clear.confirmationDialog("Move \(moveTo.map(session.serverName) ?? "") to folder", isPresented: present($moveTo), titleVisibility: .visible, presenting: moveTo) { id in
                ForEach(RailMenu.folders(for: id, in: rail), id: \.self) { folder in
                    Button(session.folderLabel(folder)) { moveInto(id, folder) }
                }
            }
        }
        .sheet(item: $settings) { settingsSheet($0) }
        .alert("Not saved", isPresented: Binding(get: { session.railError != nil }, set: { if !$0 { session.railErrorRead() } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(session.railError ?? "")
        }
    }

    // MARK: Tiles

    /// A server's tile: its picture over its initials, its count or unread
    /// dot while it is not on screen.
    @ViewBuilder
    private func serverTile(_ id: String) -> some View {
        if let s = server(id) {
            let unread = session.serverUnread(id)
            let current = place == .server(id: id)
            tileState(id, radius: LumiaTokens.radius14) {
                square(current: current || target?.highlight == id) {
                    LumiaPictureOver(url: session.pictureURL(s.iconUrl)) {
                        Text(initials(of: s.name)).font(LumiaTokens.titleMonogram.font).foregroundStyle(LumiaTokens.ink.color)
                    }
                }
                .overlay(alignment: .topTrailing) { badge(current ? 0 : Int(unread.mentions), unread: unread.unread && !current) }
            }
            .reportFrame(id)
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius14))
            .onTapGesture { place = .server(id: id) }
            .gesture(lift(id, isFolder: false))
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(s.name)
            .accessibilityValue(Replies.unreadWords(unread: unread.unread ? 1 : 0, mentions: unread.mentions))
            .accessibilityAddTraits(current ? [.isButton, .isSelected] : .isButton)
            .accessibilityAction { place = .server(id: id) }
            .accessibilityActions { actions(for: id) }
        }
    }

    /// A closed folder: one tile in its tone, its first four servers small,
    /// their mentions added up, else the unread dot. Ringed while the server
    /// on screen is in it.
    private func closedFolder(_ id: String, tone: AvatarTone, servers: [String]) -> some View {
        let counts = folderCounts(servers)
        let current = servers.contains { place == .server(id: $0) }
        return tileState(id, radius: LumiaTokens.radius14) {
            square(current: current || target?.highlight == id, fill: tone.fill.color, edge: tone.ink.color.opacity(0.15)) {
                miniTiles(servers)
            }
            .overlay(alignment: .topTrailing) { badge(Int(counts.mentions), unread: counts.unread) }
        }
        .reportFrame(id)
        .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        .onTapGesture { toggle(id) }
        .gesture(lift(id, isFolder: true))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(RailWords.folderSpoken(session.folderLabel(id)))
        .accessibilityValue(RailWords.folderValue(open: false, unread: counts.unread, mentions: counts.mentions))
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { toggle(id) }
        .accessibilityActions { actions(for: id) }
    }

    /// An open folder: its glyph, which closes it, over its servers, in a column of its tone.
    private func openFolder(_ id: String, tone: AvatarTone, servers: [String]) -> some View {
        let counts = folderCounts(servers)
        return tileState(id, radius: LumiaTokens.radius18) {
            VStack(spacing: LumiaTokens.space10) {
                Image(systemName: "folder").font(.system(size: LumiaTokens.space18, weight: .medium))
                    .foregroundStyle(tone.ink.color)
                    .frame(width: LumiaTokens.tileSize, height: LumiaTokens.rowHeight)
                    .scaleEffect(target?.highlight == id ? 1.06 : 1)
                    .reportFrame(id)
                    .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                    .onTapGesture { toggle(id) }
                    .gesture(lift(id, isFolder: true))
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(RailWords.folderSpoken(session.folderLabel(id)))
                    .accessibilityValue(RailWords.folderValue(open: true, unread: counts.unread, mentions: counts.mentions))
                    .accessibilityAddTraits(.isButton)
                    .accessibilityAction { toggle(id) }
                    .accessibilityActions { actions(for: id) }
                ForEach(servers, id: \.self) { serverTile($0) }
            }
            .padding(.top, LumiaTokens.space4)
            .padding(.bottom, LumiaTokens.space8)
            .frame(width: LumiaTokens.tileSize + LumiaTokens.space8)
            .background(tone.fill.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius18))
            .overlay {
                if target?.highlight == id {
                    RoundedRectangle(cornerRadius: LumiaTokens.radius18).strokeBorder(LumiaTokens.accent.color, lineWidth: LumiaTokens.space2)
                }
            }
        }
        .reportFrame("group:" + id)
        // Its servers are a list inside it.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(session.folderLabel(id))
    }

    /// A closed folder's first four servers, 14 across, two to a row.
    private func miniTiles(_ servers: [String]) -> some View {
        let shown = Array(servers.prefix(4))
        let rows = stride(from: 0, to: shown.count, by: 2).map { Array(shown[$0..<min($0 + 2, shown.count)]) }
        return VStack(spacing: LumiaTokens.space4) {
            ForEach(rows, id: \.self) { row in
                HStack(spacing: LumiaTokens.space4) {
                    ForEach(row, id: \.self) { miniTile($0) }
                    if row.count == 1 && shown.count > 2 { Color.clear.frame(width: LumiaTokens.space14, height: LumiaTokens.space14) }
                }
            }
        }
    }

    private func miniTile(_ id: String) -> some View {
        let s = server(id)
        return LumiaPictureOver(url: session.pictureURL(s?.iconUrl ?? "")) {
            Text(initials(of: s?.name ?? "")).font(.system(size: LumiaTokens.space6, weight: .heavy))
                .foregroundStyle(AvatarTone.of(id).ink.color)
        }
        .frame(width: LumiaTokens.space14, height: LumiaTokens.space14)
        .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.space4))
        .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.space4))
    }

    /// A tile's square: 44 across with the boards' 14 radius, in the tile's
    /// fill (ringed in the border in the dark, as the boards draw it), and
    /// the accent ring outside it, 2 off, when it is the one on screen.
    private func square<C: View>(current: Bool, fill: Color? = nil, edge: Color? = nil, @ViewBuilder content: () -> C) -> some View {
        let shape = RoundedRectangle(cornerRadius: LumiaTokens.radius14)
        return content()
            .frame(width: LumiaTokens.tileSize, height: LumiaTokens.tileSize)
            .background(fill ?? (current ? LumiaTokens.raised.color : LumiaTokens.tile.color), in: shape)
            // A server's picture takes the tile's shape.
            .clipShape(shape)
            .overlay {
                if let edge = edge ?? (scheme == .dark ? LumiaTokens.border.color : nil) { shape.strokeBorder(edge, lineWidth: 1) }
            }
            .overlay {
                if current {
                    RoundedRectangle(cornerRadius: LumiaTokens.radius18).strokeBorder(LumiaTokens.accent.color, lineWidth: LumiaTokens.space2)
                        .padding(-LumiaTokens.space4)
                }
            }
    }

    /// A tile's count over its top end, `out` past it (the boards' 4 for a
    /// server, 5 for Home), ringed in the rail's colour; with no count and
    /// something unread, the dot there instead.
    @ViewBuilder
    private func badge(_ count: Int, unread: Bool, out: CGFloat = LumiaTokens.space4) -> some View {
        if count > 0 {
            LumiaCountBadge(UInt32(count), style: .ringed)
                .alignmentGuide(.top) { $0[.top] + out }
                .alignmentGuide(.trailing) { $0[.trailing] - out }
        } else if unread {
            UnreadDot()
        }
    }

    /// How a tile looks while a long press holds it: lifted, and once it
    /// moves a dashed place where it was, its copy following the finger.
    private func tileState<C: View>(_ id: String, radius: CGFloat, @ViewBuilder content: () -> C) -> some View {
        let held = lifted == id
        let moving = held && dragged != nil
        return content()
            .scaleEffect(held && !moving ? 1.06 : 1)
            .shadow(color: held && !moving ? LumiaTokens.ink.color.opacity(0.18) : .clear, radius: LumiaTokens.space14, y: LumiaTokens.space12)
            .opacity(moving ? 0 : 1)
            .overlay {
                if moving { RoundedRectangle(cornerRadius: radius).strokeBorder(LumiaTokens.borderDashed.color, style: dashed) }
            }
            .animation(.easeOut(duration: 0.15), value: held)
    }

    private var dashed: StrokeStyle { StrokeStyle(lineWidth: 1.5, dash: [LumiaTokens.space4, LumiaTokens.space4]) }

    /// The insertion line and the tile being dragged, over the rail.
    private var dragOverlay: some View {
        ZStack {
            if let y = target?.line {
                Capsule().fill(LumiaTokens.accent.color)
                    .frame(width: LumiaTokens.space28 + LumiaTokens.space8, height: LumiaTokens.space4)
                    .position(x: LumiaTokens.railWidth / 2, y: y)
            }
            if let id = lifted, let at = dragged {
                liftedFace(id)
                    .scaleEffect(1.06)
                    .shadow(color: LumiaTokens.ink.color.opacity(0.18), radius: LumiaTokens.space14, y: LumiaTokens.space12)
                    .position(at)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    /// What follows the finger: the server's tile, or the folder closed.
    @ViewBuilder
    private func liftedFace(_ id: String) -> some View {
        if case let .folder(_, _, tone, servers)? = rail.first(where: { $0.id == id }) {
            square(current: false, fill: AvatarTone.folder(tone, id: id).fill.color, edge: AvatarTone.folder(tone, id: id).ink.color.opacity(0.15)) { miniTiles(servers) }
        } else if let s = server(id) {
            square(current: false) {
                LumiaPictureOver(url: session.pictureURL(s.iconUrl)) {
                    Text(initials(of: s.name)).font(LumiaTokens.titleMonogram.font).foregroundStyle(LumiaTokens.ink.color)
                }
            }
        }
    }

    // The account menu, in the web's and the desktop shell's order. No
    // board draws one for a phone, so it is the system's menu: the glyphs
    // are the system's for the design's user, preferences, shield and
    // sign-out, and `fixed` keeps the order when it opens upwards. Sign out
    // is last, under a rule, as it is there, and asks nothing first, as
    // there: signing in brings back all that goes. The system marks an item
    // that ends something as destructive, which is how it draws its own Sign Out.
    private var accountMenu: some View {
        Menu {
            Button { open(.profile) } label: { Label("View profile", systemImage: "person") }
            Button { open(.preferences) } label: { Label("Preferences", systemImage: "slider.horizontal.3") }
            Button { open(.devices) } label: { Label("Devices", systemImage: "shield") }
            Divider()
            Button(role: .destructive) {
                Task { await store.signOut() }
            } label: {
                Label("Sign out", systemImage: "rectangle.portrait.and.arrow.right")
            }
        } label: {
            LumiaAvatar(initials: initials(of: session.selfName), tone: .of(session.me), size: LumiaTokens.avatarSelf, picture: session.picture(of: session.me))
        }
        .menuOrder(.fixed)
        .buttonStyle(.plain)
        .accessibilityLabel(session.selfName + ", account")
    }

    // MARK: Moving

    /// The tiles as drawn, in order, for where a drag would drop.
    private var slots: [RailSlot] {
        var slots: [RailSlot] = []
        for item in rail {
            guard let frame = frames[item.id] else { continue }
            switch item {
            case .server: slots.append(RailSlot(id: item.id, frame: frame))
            case let .folder(folderId, _, _, serverIds):
                if openFolders.isOpen(folderId), let group = frames["group:" + folderId] {
                    slots.append(RailSlot(id: folderId, isFolder: true, frame: frame, group: group))
                    for id in serverIds { if let f = frames[id] { slots.append(RailSlot(id: id, folder: folderId, frame: f)) } }
                } else {
                    slots.append(RailSlot(id: folderId, isFolder: true, frame: frame))
                }
            }
        }
        return slots
    }

    /// A long press lifts a tile; moving then drags it, and letting go
    /// drops it where the line or the ring says; letting go without moving
    /// opens its menu.
    private func lift(_ id: String, isFolder: Bool) -> some Gesture {
        LongPressGesture(minimumDuration: 0.35)
            .onEnded { _ in
                lifted = id
                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
            }
            .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .named(Self.space)))
            .updating($pressing) { _, pressing, _ in pressing = true }
            .onChanged { value in
                guard case let .second(true, drag?) = value, lifted == id else { return }
                if dragged == nil {
                    guard hypot(drag.translation.width, drag.translation.height) > LumiaTokens.space8, let f = frames[id] else { return }
                    grab = CGSize(width: f.midX - drag.startLocation.x, height: f.midY - drag.startLocation.y)
                }
                let at = CGPoint(x: drag.location.x + grab.width, y: drag.location.y + grab.height)
                dragged = at
                let next = RailDropTarget.at(at.y, dragging: id, isFolder: isFolder, slots: slots, gap: LumiaTokens.space12)
                if next != target {
                    if next != nil { UISelectionFeedbackGenerator().selectionChanged() }
                    target = next
                }
            }
            .onEnded { value in
                let moved = dragged != nil
                let drop = target?.drop
                lifted = nil; dragged = nil; target = nil
                guard case .second(true, _) = value else { return }
                if moved {
                    if let drop { self.drop(id, drop) }
                } else {
                    menu = id
                }
            }
    }

    private func drop(_ id: String, _ drop: RailDrop) {
        let items = rail
        let client = session.client
        switch drop {
        case let .between(folder, before): apply(client.railMoveTo(items, id: id, folder: folder, before: before), moved: id, from: items)
        case let .onto(server): apply(client.railMerge(items, dragged: id, onto: server), moved: id, from: items)
        case let .into(folder): apply(client.railIntoFolder(items, serverID: id, folder: folder), moved: id, from: items)
        }
    }

    // MARK: The menu

    /// A tile's menu as VoiceOver's actions: Move to folder is one per folder.
    @ViewBuilder
    private func actions(for id: String) -> some View {
        ForEach(RailMenu.actions(for: id, in: rail), id: \.self) { action in
            if action == .moveToFolder {
                ForEach(RailMenu.folders(for: id, in: rail), id: \.self) { folder in
                    Button("Move to \(session.folderLabel(folder))") { moveInto(id, folder) }
                }
            } else {
                Button(action.title) { run(action, on: id) }
            }
        }
    }

    private func run(_ action: RailAction, on id: String) {
        let items = rail
        let client = session.client
        switch action {
        case .moveUp: apply(client.railStep(items, id: id, by: -1), moved: id, from: items)
        case .moveDown: apply(client.railStep(items, id: id, by: 1), moved: id, from: items)
        case .newFolder:
            let after = client.railNewFolder(items, serverID: id)
            apply(after, moved: id, from: items)
            // Made from the menu, its settings follow (not when made by a drag).
            if let folder = client.railFolderOf(after, serverID: id) {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { settings = FolderSettingsTarget(id: folder, items: after) }
            }
        case .moveToFolder: moveTo = id
        case .removeFromFolder: apply(client.railOutOfFolder(items, serverID: id), moved: id, from: items)
        case .folderSettings: settings = FolderSettingsTarget(id: id, items: items)
        case .deleteFolder:
            let label = session.folderLabel(id)
            apply(client.railDeleteFolder(items, folder: id), moved: nil, from: items)
            UIAccessibility.post(notification: .announcement, argument: "\(label) deleted")
        }
    }

    private func moveInto(_ id: String, _ folder: String) {
        let items = rail
        apply(session.client.railIntoFolder(items, serverID: id, folder: folder), moved: id, from: items)
    }

    /// An edit made: drawn at once and sent, and the move said.
    private func apply(_ after: [RailItem], moved id: String?, from before: [RailItem]) {
        guard after != before else { return }
        if let id {
            let client = session.client
            let was = client.railFolderOf(before, serverID: id)
            let now = client.railFolderOf(after, serverID: id)
            let made = now.map { folder in !before.contains { $0.id == folder } } ?? false
            let into = now != nil && now != was ? now.map { label(of: $0, in: after) } : nil
            UIAccessibility.post(notification: .announcement, argument: RailWords.moved(session.railName(id), position: client.railPositionOf(after, id: id), into: into, newFolder: made))
        }
        Task { await session.arrangeRail(after) }
    }

    private func toggle(_ folder: String) {
        var open = openFolders
        open.toggle(folder)
        openStored = open.stored
    }

    // MARK: Folders

    private func folderCounts(_ servers: [String]) -> (unread: Bool, mentions: UInt32) {
        RailCounts.folder(servers.map { id in
            place == .server(id: id) ? (false, 0) : session.serverUnread(id)
        })
    }

    private func label(of folder: String, in items: [RailItem]) -> String {
        guard case let .folder(_, name, _, servers)? = items.first(where: { $0.id == folder }) else { return "" }
        return RailWords.folderLabel(name: name, words: session.client.railFolderWords(servers.map(session.serverName)))
    }

    /// Folder settings for a folder as the rail it was opened from has it.
    private func settingsSheet(_ target: FolderSettingsTarget) -> some View {
        let entry = target.items.first { $0.id == target.id }
        guard case let .folder(_, name, tone, servers)? = entry else {
            return FolderSettingsSheet(placeholder: "", name: "", tone: .of(target.id)) { _, _ in }
        }
        return FolderSettingsSheet(placeholder: session.client.railFolderWords(servers.map(session.serverName)), name: name, tone: AvatarTone.folder(tone, id: target.id)) { name, tone in
            apply(session.client.railSetFolder(rail, folder: target.id, name: name, tone: tone.rawValue), moved: nil, from: rail)
        }
    }
}

/// The folder whose settings are open, with the rail it was opened from
/// (a folder just made from the menu is not in the snapshot yet).
struct FolderSettingsTarget: Identifiable {
    let id: String
    let items: [RailItem]
}

/// Folder settings (the "Server folders" board): the name, its servers'
/// names as the placeholder, the nine tones as swatches, Cancel and Save.
struct FolderSettingsSheet: View {
    @Environment(\.dismiss) private var dismiss
    let placeholder: String
    @State var name: String
    @State var tone: AvatarTone
    let save: (String, AvatarTone) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            Text("Folder settings").font(LumiaTokens.titleChannel.font).foregroundStyle(LumiaTokens.ink.color)
                .accessibilityAddTraits(.isHeader)
            VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                LumiaField("Name", text: $name, placeholder: placeholder, spoken: "Name")
                Text(RailWords.nameCaption).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
            }
            VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                Text("Colour").font(LumiaTokens.textLabel.font).foregroundStyle(LumiaTokens.inkSecondary.color)
                HStack(spacing: LumiaTokens.space8) {
                    ForEach(AvatarTone.allCases, id: \.self) { swatch($0) }
                }
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Colour")
            }
            HStack(spacing: LumiaTokens.space8) {
                Spacer(minLength: 0)
                Button("Cancel") { dismiss() }.buttonStyle(.lumiaSecondary)
                Button("Save") {
                    save(name, tone)
                    dismiss()
                }
                .buttonStyle(.lumiaPrimary)
            }
            Spacer(minLength: 0)
        }
        .padding(LumiaTokens.space24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .presentationDetents([.medium])
        .presentationBackground(LumiaTokens.surface.color)
        .onChange(of: name) { _, now in
            if now.count > RailWords.nameMax { name = String(now.prefix(RailWords.nameMax)) }
        }
    }

    private func swatch(_ t: AvatarTone) -> some View {
        let chosen = t == tone
        return Button { tone = t } label: {
            Circle().fill(t.fill.color)
                .overlay { if !chosen { Circle().strokeBorder(LumiaTokens.lineStrong.color, lineWidth: 1) } }
                .overlay {
                    if chosen { Image(systemName: "checkmark").font(.system(size: LumiaTokens.space14, weight: .bold)).foregroundStyle(t.ink.color) }
                }
                .frame(width: LumiaTokens.space28, height: LumiaTokens.space28)
                .overlay {
                    if chosen { Circle().strokeBorder(LumiaTokens.accent.color, lineWidth: LumiaTokens.space2).padding(-LumiaTokens.space4) }
                }
                .padding(LumiaTokens.space2)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(RailWords.toneName(t))
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }
}

/// Where each rail tile is drawn, by id, in the rail's space.
private struct RailFrames: PreferenceKey {
    static var defaultValue: [String: CGRect] = [:]
    static func reduce(value: inout [String: CGRect], nextValue: () -> [String: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

private extension View {
    func reportFrame(_ id: String) -> some View {
        background(GeometryReader { g in
            Color.clear.preference(key: RailFrames.self, value: [id: g.frame(in: .named("rail"))])
        })
    }
}

/// A binding that is presented while the value is there, and empties it when dismissed.
private func present<T>(_ value: Binding<T?>) -> Binding<Bool> {
    Binding(get: { value.wrappedValue != nil }, set: { if !$0 { value.wrappedValue = nil } })
}
