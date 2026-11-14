// The phone's chrome, after the iOS boards: the rail beside a sidebar as
// one screen, and the floating panel a pushed screen draws itself in
// (a header with the back button, a title and subtitle, and actions).
import LumiaFFI
import LumiaKit
import SwiftUI

/// Where the shell is: Home, or a server.
enum Place: Hashable {
    case home
    case server(id: String)
}

/// A screen pushed over the rail and sidebar.
enum Screen: Hashable {
    case conversation(id: String)
    case channel(id: String)
    case requests
    case devices
    case profile
    case preferences
    case newMessage
    case addServer
    case newChannel(serverID: String)
    case invite(serverID: String)
    case serverSettings(serverID: String)
    case browse(serverID: String)
    case channelMembers(channelID: String)
    case addPeople(channelID: String)
    case newServerDM(serverID: String)
    case yourSettings(serverID: String)
    /// A server's Mentions page, or Home's (every server's) for nil.
    case mentions(serverID: String?)
}

struct PhoneRail: View {
    @Environment(AppStore.self) private var store
    let session: LumiaSession
    @Binding var place: Place
    let add: () -> Void
    /// Opens one of the account's own screens, from the menu on the avatar.
    let open: (Screen) -> Void

    private var servers: [ServerSummary] { session.snapshot?.servers ?? [] }

    var body: some View {
        VStack(spacing: LumiaTokens.space12) {
            // Home's count: the requests that wait and the personal conversations' unread messages.
            tile(current: place == .home, badge: (session.snapshot?.pendingRequests.count ?? 0) + Int(session.personalMentions), badgeOut: LumiaTokens.space10 / 2, label: "Home") {
                place = .home
            } content: {
                Image("Logo").resizable().scaledToFit().padding(LumiaTokens.space6)
            }
            Rectangle().fill(LumiaTokens.railDivider.color).frame(width: 24, height: 1)
            ForEach(servers, id: \.serverId) { s in
                // What is unread in a server not on screen (the board's): its
                // count for this account on the tile, else a dot at its top end.
                let unread = session.serverUnread(s.serverId)
                let current = place == .server(id: s.serverId)
                tile(current: current, badge: current ? 0 : Int(unread.mentions), unread: unread.unread && !current, label: s.name, value: Replies.unreadWords(unread: unread.unread ? 1 : 0, mentions: unread.mentions)) {
                    place = .server(id: s.serverId)
                } content: {
                    // The server's picture over its initials, when it has one (architecture 6.7).
                    LumiaPictureOver(url: session.pictureURL(s.iconUrl)) {
                        Text(initials(of: s.name)).font(LumiaTokens.titleMonogram.font).foregroundStyle(LumiaTokens.ink.color)
                    }
                }
            }
            tile(current: false, badge: 0, label: "Add a server", action: add) {
                Image(systemName: "plus").font(.system(size: 18, weight: .medium)).foregroundStyle(LumiaTokens.inkSecondary.color)
            }
            Spacer()
            // The account menu, in the web's and the desktop shell's order.
            // No board draws one for a phone, so it is the system's menu:
            // the glyphs are the system's for the design's user, preferences,
            // shield and sign-out, and `fixed` keeps the order when it opens
            // upwards. Sign out is last, under a rule, as it is there, and
            // asks nothing first, as there: signing in brings back all that
            // goes. The system marks an item that ends something as
            // destructive, which is how it draws its own Sign Out.
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
                LumiaAvatar(initials: initials(of: session.selfName), tone: .of(session.me), size: LumiaTokens.avatarSelf)
            }
            .menuOrder(.fixed)
            .buttonStyle(.plain)
            .accessibilityLabel(session.selfName + ", account")
        }
        .padding(.top, LumiaTokens.space8)
        .padding(.bottom, LumiaTokens.space12)
        .frame(width: LumiaTokens.railWidth)
        .frame(maxHeight: .infinity)
        .background(LumiaTokens.rail.color.ignoresSafeArea())
    }

    /// A rail tile. Its count sits over its top end, `badgeOut` past it (the
    /// boards' 4 for a server, 5 for Home), ringed in the rail's colour; a
    /// server with something unread and no count has the dot there instead.
    private func tile<C: View>(current: Bool, badge: Int, badgeOut: CGFloat = LumiaTokens.space4, unread: Bool = false, label: String, value: String = "", action: @escaping () -> Void, @ViewBuilder content: () -> C) -> some View {
        Button(action: action) {
            content()
                .frame(width: LumiaTokens.tileSize, height: LumiaTokens.tileSize)
                .background(current ? LumiaTokens.tabActive.color : LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: current ? LumiaTokens.radius14 : LumiaTokens.radiusRound))
                // A server's picture takes the tile's shape.
                .clipShape(RoundedRectangle(cornerRadius: current ? LumiaTokens.radius14 : LumiaTokens.radiusRound))
                .overlay(RoundedRectangle(cornerRadius: current ? LumiaTokens.radius14 : LumiaTokens.radiusRound).stroke(current ? LumiaTokens.accent.color : .clear, lineWidth: 2))
                .overlay(alignment: .topTrailing) {
                    if badge > 0 {
                        LumiaCountBadge(UInt32(badge), style: .ringed)
                            .alignmentGuide(.top) { $0[.top] + badgeOut }
                            .alignmentGuide(.trailing) { $0[.trailing] - badgeOut }
                    } else if unread {
                        UnreadDot()
                    }
                }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityValue(value)
        .accessibilityAddTraits(current ? .isSelected : [])
    }
}

/// A server's unread mark on the rail: a dot at its tile's top end, 11
/// across with a 2-point ring in the rail's colour.
struct UnreadDot: View {
    var body: some View {
        Circle().fill(LumiaTokens.unread.color)
            .overlay(Circle().strokeBorder(LumiaTokens.rail.color, lineWidth: LumiaTokens.space2))
            .frame(width: LumiaTokens.space10 + 1, height: LumiaTokens.space10 + 1)
            .accessibilityHidden(true)
    }
}

/// The sidebar's header: a title, a line under it, and one action.
struct SideHeader: View {
    let title: String
    var meta: String?
    let action: (icon: String, label: String, run: () -> Void)?

    var body: some View {
        HStack(spacing: LumiaTokens.space4) {
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(LumiaTokens.titleServer.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                if let meta { Text(meta).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1) }
            }
            .frame(minHeight: LumiaTokens.tileSize)
            Spacer(minLength: 0)
            if let action {
                Button(action: action.run) {
                    Image(systemName: action.icon).font(.system(size: 17))
                        .foregroundStyle(LumiaTokens.iconStrong.color)
                        .frame(width: LumiaTokens.controlHeight, height: LumiaTokens.controlHeight)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(action.label)
            }
        }
        // The phone boards' header: 4 over it and 8 under it, its words 20 in (its row's 12 and
        // the title's own 8), so what follows does not touch the address under the name.
        .padding(.leading, LumiaTokens.space12 + LumiaTokens.space8)
        .padding(.trailing, LumiaTokens.space6)
        .padding(.top, LumiaTokens.space4)
        .padding(.bottom, LumiaTokens.space8)
    }
}

/// A quick tile (Home's Requests, Devices and New; a server's Mentions): the
/// boards' 58-high tile, three to a row, ringed in the edge's colour, its
/// count as the small badge at its top end. `spoken`, where given, is what
/// assistive technology hears after the name as its value; else the count.
struct Tile: View {
    let label: String
    let icon: String
    var count = 0
    var spoken: String?
    let action: () -> Void

    /// The boards' quick tile: 58 high.
    static let height = LumiaTokens.space40 + LumiaTokens.space18

    var body: some View {
        Button(action: action) {
            VStack(spacing: LumiaTokens.space4) {
                Image(systemName: icon).font(.system(size: LumiaTokens.space18 - 1))
                Text(label).font(LumiaTokens.textSection.font)
            }
            .foregroundStyle(LumiaTokens.inkBody.color)
            .frame(maxWidth: .infinity).frame(height: Self.height)
            .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.edge.color))
            .overlay(alignment: .topTrailing) {
                if count > 0 {
                    LumiaCountBadge(UInt32(count), style: .small)
                        .padding(.top, LumiaTokens.space6)
                        .padding(.trailing, LumiaTokens.space6 + 1)
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius14))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(spoken != nil || count == 0 ? label : "\(label), \(count)")
        .accessibilityValue(spoken ?? "")
    }
}

/// A floating panel: the main pane's card, pushed over the rail and
/// sidebar, with its own header (the system bar is hidden).
struct PanelScreen<Lead: View, Content: View>: View {
    @Environment(\.dismiss) private var dismiss
    let back: String
    let title: String
    var subtitle: String?
    @ViewBuilder let lead: () -> Lead
    var actions: [(icon: String, label: String, run: () -> Void)] = []
    @ViewBuilder let content: () -> Content

    init(back: String, title: String, subtitle: String? = nil, actions: [(icon: String, label: String, run: () -> Void)] = [], @ViewBuilder lead: @escaping () -> Lead = { EmptyView() }, @ViewBuilder content: @escaping () -> Content) {
        self.back = back; self.title = title; self.subtitle = subtitle; self.actions = actions; self.lead = lead; self.content = content
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: LumiaTokens.space8) {
                Button { dismiss() } label: {
                    Image(systemName: "chevron.left").font(.system(size: 20, weight: .semibold))
                        .foregroundStyle(LumiaTokens.iconStrong.color)
                        .frame(width: LumiaTokens.controlHeight, height: LumiaTokens.controlHeight)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Back to \(back)")
                lead()
                VStack(alignment: .leading, spacing: 0) {
                    Text(title).font(LumiaTokens.titleChannel.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                    if let subtitle { Text(subtitle).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1) }
                }
                Spacer(minLength: 0)
                ForEach(Array(actions.enumerated()), id: \.offset) { _, a in
                    Button(action: a.run) {
                        Image(systemName: a.icon).font(.system(size: 17))
                            .foregroundStyle(LumiaTokens.iconStrong.color)
                            .frame(width: LumiaTokens.controlHeight, height: LumiaTokens.controlHeight)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(a.label)
                }
            }
            .padding(.leading, LumiaTokens.space4)
            .padding(.trailing, LumiaTokens.space6)
            .frame(minHeight: LumiaTokens.headerHeight)
            Divider().overlay(LumiaTokens.line.color)
            content()
        }
        .background(LumiaTokens.surface.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius18))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius18).stroke(LumiaTokens.panelEdge.color))
        .padding(.horizontal, LumiaTokens.space8)
        .padding(.bottom, LumiaTokens.space8)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(LumiaTokens.bg.color.ignoresSafeArea())
        .toolbar(.hidden, for: .navigationBar)
    }
}

/// A section title in a list.
struct SectionTitle: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(text).font(LumiaTokens.textSection.font).foregroundStyle(LumiaTokens.inkMuted.color)
            .padding(.horizontal, LumiaTokens.space10).padding(.top, LumiaTokens.space4).padding(.bottom, LumiaTokens.space6)
    }
}

/// When a message was sent, as a row shows it: the time today, the day otherwise.
func rowTime(ofMicros micros: Int64) -> String {
    let date = Date(timeIntervalSince1970: Double(micros) / 1_000_000)
    return Calendar.current.isDateInToday(date) ? time(ofMicros: micros) : day(ofMicros: micros)
}
