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
            tile(current: place == .home, badge: session.snapshot?.pendingRequests.count ?? 0, label: "Home") {
                place = .home
            } content: {
                Image("Logo").resizable().scaledToFit().padding(LumiaTokens.space6)
            }
            Rectangle().fill(LumiaTokens.railDivider.color).frame(width: 24, height: 1)
            ForEach(servers, id: \.serverId) { s in
                tile(current: place == .server(id: s.serverId), badge: 0, label: s.name) {
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

    private func tile<C: View>(current: Bool, badge: Int, label: String, action: @escaping () -> Void, @ViewBuilder content: () -> C) -> some View {
        Button(action: action) {
            content()
                .frame(width: LumiaTokens.tileSize, height: LumiaTokens.tileSize)
                .background(current ? LumiaTokens.tabActive.color : LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: current ? LumiaTokens.radius14 : LumiaTokens.radiusRound))
                // A server's picture takes the tile's shape.
                .clipShape(RoundedRectangle(cornerRadius: current ? LumiaTokens.radius14 : LumiaTokens.radiusRound))
                .overlay(RoundedRectangle(cornerRadius: current ? LumiaTokens.radius14 : LumiaTokens.radiusRound).stroke(current ? LumiaTokens.accent.color : .clear, lineWidth: 2))
                .overlay(alignment: .topTrailing) {
                    if badge > 0 { CountBadge(badge).offset(x: 4, y: -4) }
                }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityAddTraits(current ? .isSelected : [])
    }
}

struct CountBadge: View {
    let count: Int
    init(_ count: Int) { self.count = count }
    var body: some View {
        Text("\(count)").font(LumiaTokens.textCount.font).foregroundStyle(LumiaTokens.onAccent.color)
            .padding(.horizontal, LumiaTokens.space4).frame(minWidth: 18, minHeight: 18)
            .background(LumiaTokens.unread.color, in: Capsule())
    }
}

/// The sidebar's header: a title, a line under it, and one action.
struct SideHeader: View {
    let title: String
    var meta: String?
    let action: (icon: String, label: String, run: () -> Void)?

    var body: some View {
        HStack(spacing: LumiaTokens.space4) {
            VStack(alignment: .leading, spacing: 0) {
                Text(title).font(LumiaTokens.titleServer.font).foregroundStyle(LumiaTokens.ink.color).lineLimit(1)
                if let meta { Text(meta).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkSecondary.color).lineLimit(1) }
            }
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
        .padding(.leading, LumiaTokens.space16)
        .padding(.trailing, LumiaTokens.space6)
        .padding(.vertical, LumiaTokens.space4)
    }
}

/// The Requests / Devices / New message tiles.
struct Tile: View {
    let label: String
    let icon: String
    var count = 0
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: LumiaTokens.space4) {
                Image(systemName: icon).font(.system(size: 16))
                Text(label).font(LumiaTokens.textCompact.font)
            }
            .foregroundStyle(LumiaTokens.inkBody.color)
            .frame(maxWidth: .infinity).frame(height: 58)
            .background(LumiaTokens.tile.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius14))
            .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).stroke(LumiaTokens.line.color))
            .overlay(alignment: .topTrailing) {
                if count > 0 { CountBadge(count).padding(LumiaTokens.space6) }
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(count > 0 ? "\(label), \(count)" : label)
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
