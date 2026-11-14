// The server rail's order and folders (architecture 6.9). The core keeps the
// rules: the rail ready to draw is the snapshot's, every edit is one of its
// `rail…` calls, and `arrangeRail` keeps the result. What is the phone's own
// is here, as values the tests cover: a folder's count and words, which of
// its folders this phone has open, a tile's menu, where a drag would drop,
// and what is said of a move. The session draws an edit at once and sends
// it; a refusal puts the snapshot's rail back and is kept to be said.
import CoreGraphics
import Foundation
import LumiaFFI

public extension RailItem {
    /// The id a tile goes by: its server's, or its folder's.
    var id: String {
        switch self {
        case let .server(serverId): serverId
        case let .folder(folderId, _, _, _): folderId
        }
    }

    /// The servers it stands for: itself, or a folder's.
    var serverIDs: [String] {
        switch self {
        case let .server(serverId): [serverId]
        case let .folder(_, _, _, serverIds): serverIds
        }
    }

    var isFolder: Bool { if case .folder = self { true } else { false } }
}

/// Where a tile stands: on the rail (`folder` nil) or in a folder, its place
/// there from 0, and how many share it.
public struct RailPlace: Equatable, Sendable {
    public let folder: String?
    public let index: Int
    public let count: Int

    public static func of(_ id: String, in items: [RailItem]) -> RailPlace? {
        if let at = items.firstIndex(where: { $0.id == id }) { return RailPlace(folder: nil, index: at, count: items.count) }
        for case let .folder(folderID, _, _, servers) in items {
            if let at = servers.firstIndex(of: id) { return RailPlace(folder: folderID, index: at, count: servers.count) }
        }
        return nil
    }

    public var isFirst: Bool { index == 0 }
    public var isLast: Bool { index == count - 1 }
}

/// A closed folder's mark: its servers' mentions added up, else whether any is unread.
public enum RailCounts {
    public static func folder(_ servers: [(unread: Bool, mentions: UInt32)]) -> (unread: Bool, mentions: UInt32) {
        (servers.contains { $0.unread }, servers.reduce(0) { $0 + $1.mentions })
    }
}

/// What the rail says, and what is said of it.
public enum RailWords {
    /// A folder's name, else its servers' (the core's `railFolderWords`).
    public static func folderLabel(name: String, words: String) -> String {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? words : name
    }

    /// What assistive technology calls a folder's tile.
    public static func folderSpoken(_ label: String) -> String { "Folder, \(label)" }

    /// A folder tile's value: open or not, then its servers' counts.
    public static func folderValue(open: Bool, unread: Bool, mentions: UInt32) -> String {
        let counts = Replies.unreadWords(unread: unread ? 1 : 0, mentions: mentions)
        return [open ? "expanded" : "collapsed", counts].filter { !$0.isEmpty }.joined(separator: ", ")
    }

    /// What is said once a tile has moved: into a folder it was not in, or
    /// to its place where it now is.
    public static func moved(_ name: String, position: UInt32?, into folder: String?, newFolder: Bool) -> String {
        if newFolder { return "\(name) moved into a new folder" }
        if let folder { return "\(name) moved into \(folder)" }
        guard let position else { return "\(name) moved" }
        return "\(name) moved to position \(position)"
    }

    /// A tone as the Folder settings' swatches name it.
    public static func toneName(_ tone: AvatarTone) -> String { tone.rawValue.prefix(1).uppercased() + tone.rawValue.dropFirst() }

    /// What Folder settings' field says under it.
    public static let nameCaption = "Without a name it reads as its servers."
    /// The most characters a folder's name keeps, once trimmed (the core's).
    public static let nameMax = 50
}

public extension AvatarTone {
    /// A folder's tone by its name, else the one its id hashes to.
    static func folder(_ tone: String, id: String) -> AvatarTone { AvatarTone(rawValue: tone) ?? .of(id) }
}

/// The folders this phone has open, kept on the device as one string of
/// ids (whether a folder is open is not synced, architecture 6.9).
public struct OpenFolders: Equatable, Sendable {
    public private(set) var ids: Set<String>
    /// The key it is kept under in `UserDefaults`.
    public static let key = "rail_open_folders"

    public init(stored: String) {
        ids = Set(stored.split(separator: ",").map(String.init).filter { !$0.isEmpty })
    }

    public var stored: String { ids.sorted().joined(separator: ",") }

    public func isOpen(_ folderID: String) -> Bool { ids.contains(folderID) }

    public mutating func toggle(_ folderID: String) {
        if ids.contains(folderID) { ids.remove(folderID) } else { ids.insert(folderID) }
    }

    /// Only the folders the rail still has, so a deleted one is not kept for ever.
    public func kept(in items: [RailItem]) -> OpenFolders {
        var next = self
        next.ids.formIntersection(items.filter(\.isFolder).map(\.id))
        return next
    }
}

/// One of a tile's menu items (a long press let go, or VoiceOver's actions).
public enum RailAction: Hashable, Sendable {
    case moveUp, moveDown, newFolder, moveToFolder, removeFromFolder, folderSettings, deleteFolder

    public var title: String {
        switch self {
        case .moveUp: "Move up"
        case .moveDown: "Move down"
        case .newFolder: "New folder"
        case .moveToFolder: "Move to folder"
        case .removeFromFolder: "Remove from folder"
        case .folderSettings: "Folder settings…"
        case .deleteFolder: "Delete folder"
        }
    }

    /// The system symbol drawn beside it, for the board's glyph.
    public var symbol: String {
        switch self {
        case .moveUp: "arrow.up"
        case .moveDown: "arrow.down"
        case .newFolder, .moveToFolder: "folder.badge.plus"
        case .removeFromFolder: "folder.badge.minus"
        case .folderSettings: "slider.horizontal.3"
        case .deleteFolder: "trash"
        }
    }
}

public enum RailMenu {
    /// A tile's menu in the boards' order. A server's: Move up, Move down
    /// (each where it can go), New folder, Move to folder (when there is
    /// another), Remove from folder (in one). A folder's: Move up, Move
    /// down, Folder settings…, Delete folder.
    public static func actions(for id: String, in items: [RailItem]) -> [RailAction] {
        guard let place = RailPlace.of(id, in: items) else { return [] }
        var actions: [RailAction] = []
        if !place.isFirst { actions.append(.moveUp) }
        if !place.isLast { actions.append(.moveDown) }
        if items.contains(where: { $0.id == id && $0.isFolder }) {
            return actions + [.folderSettings, .deleteFolder]
        }
        actions.append(.newFolder)
        if !folders(for: id, in: items).isEmpty { actions.append(.moveToFolder) }
        if place.folder != nil { actions.append(.removeFromFolder) }
        return actions
    }

    /// The folders a server may be moved to: every one but its own.
    public static func folders(for serverID: String, in items: [RailItem]) -> [String] {
        let own = RailPlace.of(serverID, in: items)?.folder
        return items.filter { $0.isFolder && $0.id != own }.map(\.id)
    }
}

/// A tile as drawn, for working out where a drag would drop: in layout
/// order, an open folder's glyph (`group` its whole column) before its
/// servers (`folder` its id).
public struct RailSlot: Equatable, Sendable {
    public let id: String
    public let folder: String?
    public let isFolder: Bool
    public let frame: CGRect
    public let group: CGRect?

    public init(id: String, folder: String? = nil, isFolder: Bool = false, frame: CGRect, group: CGRect? = nil) {
        self.id = id; self.folder = folder; self.isFolder = isFolder; self.frame = frame; self.group = group
    }

    /// What it takes up on the rail: an open folder's column, else its tile.
    var outer: CGRect { group ?? frame }
}

/// Where a drag would drop (architecture 6.9): between tiles, onto a
/// server (a folder of the two), or into a folder.
public enum RailDrop: Equatable, Sendable {
    /// In `folder`, or on the rail for nil, before `before` or at the end.
    case between(folder: String?, before: String?)
    case onto(server: String)
    case into(folder: String)
}

public struct RailDropTarget: Equatable, Sendable {
    public let drop: RailDrop
    /// Where the insertion line is drawn, for a drop between tiles.
    public let line: CGFloat?
    /// The tile drawn as the one dropped onto.
    public let highlight: String?

    /// Where a tile dragged to `y` would drop. Over a tile, its top or
    /// bottom quarter is between and its middle half onto; a folder drags
    /// whole and only between the rail's own tiles. Over itself, nothing.
    public static func at(_ y: CGFloat, dragging id: String, isFolder: Bool, slots: [RailSlot], gap: CGFloat) -> RailDropTarget? {
        let candidates = isFolder ? slots.filter { $0.folder == nil }.map { RailSlot(id: $0.id, isFolder: $0.isFolder, frame: $0.outer) } : slots
        guard let last = candidates.last(where: { $0.folder == nil }) else { return nil }
        guard let at = candidates.firstIndex(where: { y < $0.frame.maxY + gap / 2 }) else {
            return RailDropTarget(drop: .between(folder: nil, before: nil), line: last.outer.maxY + gap / 2, highlight: nil)
        }
        let c = candidates[at]
        if c.id == id || c.folder == id { return nil }
        let f = (y - c.frame.minY) / max(c.frame.height, 1)
        // Just after the tile dragged is where it is already.
        let stays = candidates[..<at].last { $0.folder == c.folder }?.id == id
        let before = stays ? nil : RailDropTarget(drop: .between(folder: c.folder, before: c.id), line: c.outer.minY - gap / 2, highlight: nil)
        func after() -> RailDropTarget? {
            // Past an open folder's glyph is the head of its column.
            if c.group != nil, let first = candidates[(at + 1)...].first(where: { $0.folder == c.id }) {
                return first.id == id ? nil : RailDropTarget(drop: .between(folder: c.id, before: first.id), line: first.frame.minY - gap / 2, highlight: nil)
            }
            if let next = candidates[(at + 1)...].first(where: { $0.folder == c.folder }) {
                if next.id == id { return nil }
                return RailDropTarget(drop: .between(folder: c.folder, before: next.id), line: c.outer.maxY + gap / 2, highlight: nil)
            }
            return RailDropTarget(drop: .between(folder: c.folder, before: nil), line: c.outer.maxY + gap / 2, highlight: nil)
        }
        if f < 0.25 { return before }
        if f > 0.75 { return after() }
        if isFolder { return f < 0.5 ? before : after() }
        return RailDropTarget(drop: c.isFolder ? .into(folder: c.id) : .onto(server: c.id), line: nil, highlight: c.id)
    }
}

// MARK: The session's side

public extension LumiaSession {
    /// The rail as drawn: an edit on its way, else the snapshot's.
    var rail: [RailItem] { railSending ?? snapshot?.rail ?? [] }

    /// A server's name, for its tile and what is said of it.
    func serverName(_ serverID: String) -> String {
        snapshot?.servers.first { $0.serverId == serverID }?.name ?? ""
    }

    /// A folder's name, else its servers' names as the core puts them.
    func folderLabel(_ folderID: String) -> String {
        guard case let .folder(_, name, _, servers)? = rail.first(where: { $0.id == folderID }) else { return "" }
        return RailWords.folderLabel(name: name, words: client.railFolderWords(servers.map(serverName)))
    }

    /// The name of a tile, server or folder.
    func railName(_ id: String) -> String {
        rail.contains { $0.id == id && $0.isFolder } ? folderLabel(id) : serverName(id)
    }

    /// Keeps `items` as the rail: drawn at once, then sent whole. The
    /// snapshot it answers with is held; a refusal draws the snapshot's
    /// rail again and is kept in `railError`. Nothing is sent for a rail as it is.
    func arrangeRail(_ items: [RailItem]) async {
        guard items != rail else { return }
        railEdits += 1
        let edit = railEdits
        railSending = items
        do {
            let snap = try await client.arrangeRail(items: items)
            guard edit == railEdits else { return }
            take(snap)
            railSending = nil
        } catch {
            guard edit == railEdits else { return }
            railSending = nil
            railError = error.lumiaMessage
        }
    }

    /// The refusal has been said.
    func railErrorRead() { railError = nil }
}
