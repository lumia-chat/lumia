// Preferences: how Lumia looks on this phone, after the design canvas's
// "Preferences · Light" and "Preferences · Dark" boards (2026-11-16),
// folded for a phone as the other panels are: one column, Appearance's
// three pictures of the app in a row, then Messages with its preview first,
// so it is in view while a preset is chosen, the three preset cards in a
// row, "Custom: …" when none matches, the outlined group of settings each
// chosen among a few by a compact segmented control, and the outlined
// group of switches. Each choice takes effect as it is made and is kept on
// this phone (`Preferences` in LumiaKit); nothing here is the account's or
// is sent to the instance. No scroll bars row: a phone's indicators are
// the system's. The words are LumiaKit's `PreferenceWords`, `MessageSetting`
// and `MessageSwitch`, the same as every client's.
import LumiaKit
import SwiftUI

struct PreferencesScreen: View {
    @Environment(Preferences.self) private var preferences
    @ScaledMetric(relativeTo: .body) private var systemText = LumiaTokens.textMessage.size

    var body: some View {
        PanelScreen(back: "Home", title: PreferenceWords.title, subtitle: PreferenceWords.topic) {
            ScrollViewReader { proxy in
                ScrollView { page }
                #if DEBUG
                    // A look further down the page with no finger to scroll it
                    // (`LUMIA_DEV_SCROLL`): `preview`, the preview's end over the
                    // presets; `presets`, from them on; `switches`, the page's end.
                    .onAppear {
                        switch ProcessInfo.processInfo.environment["LUMIA_DEV_SCROLL"] {
                        case "preview": proxy.scrollTo("presets", anchor: .bottom)
                        case "presets": proxy.scrollTo("presets", anchor: .top)
                        case "switches": proxy.scrollTo("switches", anchor: .bottom)
                        default: break
                        }
                    }
                #endif
            }
        }
    }

    private var page: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space40) {
            PreferenceSection(title: "Appearance", subtitle: PreferenceWords.appearance) {
                ChoiceTiles(label: "Appearance") {
                    ForEach(Appearance.allCases, id: \.self) { appearance in
                        ChoiceTile(name: appearance.label, chosen: preferences.appearance == appearance) {
                            preferences.appearance = appearance
                        } sketch: {
                            AppearanceSketch(appearance: appearance)
                        }
                    }
                }
            }
            PreferenceSection(title: "Messages", subtitle: PreferenceWords.messages) {
                // As the conversations draw them: the font size on top of the system's text size.
                MessagesPreview(measures: preferences.measures(system: systemText / LumiaTokens.textMessage.size), images: preferences.images, embeds: preferences.embeds)
                    .padding(.bottom, LumiaTokens.space8)
                // None lit when the settings below match no preset.
                ChoiceTiles(label: "Preset") {
                    ForEach(MessagePreset.allCases, id: \.self) { preset in
                        ChoiceTile(name: preset.label, caption: preset.caption, chosen: preferences.preset == preset) {
                            preferences.preset = preset
                        } sketch: {
                            PresetSketch(preset: preset)
                        }
                    }
                }
                .id("presets")
                if preferences.preset == nil { CustomLine() }
                OutlinedGroup(count: MessageSetting.shown(preferences.layout).count) { index in
                    settingRow(MessageSetting.shown(preferences.layout)[index])
                }
                OutlinedGroup(count: MessageSwitch.allCases.count) { index in
                    let setting = MessageSwitch.allCases[index]
                    SwitchRow(setting: setting, on: Binding { preferences.isOn(setting) } set: { preferences.set(setting, on: $0) })
                }
                .id("switches")
            }
        }
        .padding(.horizontal, LumiaTokens.space16)
        .padding(.top, LumiaTokens.space24)
        .padding(.bottom, LumiaTokens.space40)
    }

    /// One of the settings chosen among a few: its name and caption, and
    /// its segments beside them, or under them where they do not fit beside
    /// (the font sizes, on a phone).
    private func settingRow(_ setting: MessageSetting) -> some View {
        let chosen = Bindable(preferences)
        // The font sizes decide alone: theirs is a different width.
        let reserve = setting == .fontSize ? 0 : CompactSegments<Int>.widest([MessageLayout.allCases.map(\.label), MessageSpacing.allCases.map(\.label), AvatarSize.allCases.map(\.label)])
        return SettingRowLayout(gap: LumiaTokens.space16, under: LumiaTokens.space10, least: SettingRowLayout.leastWords, reserve: reserve) {
            RowWords(name: setting.name, caption: setting.caption)
                .accessibilityHidden(true)
            switch setting {
            case .layout:
                CompactSegments(label: setting.name, segments: MessageLayout.allCases.map { .init(id: $0, label: $0.label) }, selected: chosen.layout)
            case .spacing:
                CompactSegments(label: setting.name, segments: MessageSpacing.allCases.map { .init(id: $0, label: $0.label) }, selected: chosen.spacing)
            case .avatars:
                CompactSegments(label: setting.name, segments: AvatarSize.allCases.map { .init(id: $0, label: $0.label) }, selected: chosen.avatars)
            case .fontSize:
                // Between a small A and a large one, which say nothing to assistive technology.
                HStack(spacing: LumiaTokens.space8) {
                    Text("A").font(LumiaTokens.textCount.font).foregroundStyle(LumiaTokens.inkMuted.color).accessibilityHidden(true)
                    CompactSegments(label: setting.name, segments: FontSize.choices.map { .init(id: $0, label: $0.label, spoken: "\($0.points) points") }, selected: chosen.fontSize, width: CompactSegments<FontSize>.numberWidth)
                    Text("A").font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkMuted.color).accessibilityHidden(true)
                }
            }
        }
        .padding(.vertical, LumiaTokens.space14)
        .padding(.horizontal, LumiaTokens.space16)
    }
}

// MARK: Sections and groups

/// A section's heading and the line under it, over what it holds.
private struct PreferenceSection<Content: View>: View {
    let title: String
    let subtitle: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space16) {
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                Text(title).font(LumiaTokens.titleServer.font).foregroundStyle(LumiaTokens.ink.color)
                    .accessibilityAddTraits(.isHeader)
                Text(subtitle).font(LumiaTokens.textCompact.font).foregroundStyle(LumiaTokens.inkMuted.color)
                    .fixedSize(horizontal: false, vertical: true)
            }
            content()
        }
    }
}

/// Rows in a rounded outline, a hairline between each two.
private struct OutlinedGroup<Row: View>: View {
    let count: Int
    @ViewBuilder let row: (Int) -> Row

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(0..<count, id: \.self) { index in
                if index > 0 { LumiaTokens.line.color.frame(height: 1) }
                row(index)
            }
        }
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius14).strokeBorder(LumiaTokens.lineStrong.color))
    }
}

/// A setting's name over what it does.
private struct RowWords: View {
    let name: String
    let caption: String

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space2) {
            Text(name).font(LumiaTokens.textMember.font).foregroundStyle(LumiaTokens.ink.color)
            Text(caption).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A setting's row: its words at the start and its control at the end,
/// both on the row's middle; or, when the control would leave the words
/// less than `least`, the words over the control. The control is taken to
/// be at least `reserve` wide, so the rows of a group decide alike and
/// their words wrap in one column: on a narrow phone Layout, Spacing and
/// Avatars all stand under their words, never some beside and some under.
private struct SettingRowLayout: Layout {
    let gap: CGFloat
    let under: CGFloat
    let least: CGFloat
    var reserve: CGFloat = 0

    /// Narrower than this the captions would wrap a word to a line.
    static let leastWords = LumiaTokens.space40 * 2 + LumiaTokens.space16

    private func arranged(_ subviews: Subviews, width: CGFloat?) -> (beside: Bool, words: CGSize, control: CGSize, width: CGFloat) {
        let control = subviews[1].sizeThatFits(.unspecified)
        guard let width else {
            let words = subviews[0].sizeThatFits(.unspecified)
            return (true, words, control, words.width + gap + control.width)
        }
        let room = width - max(control.width, reserve) - gap
        if room >= least {
            return (true, subviews[0].sizeThatFits(ProposedViewSize(width: room, height: nil)), control, width)
        }
        return (false, subviews[0].sizeThatFits(ProposedViewSize(width: width, height: nil)), control, width)
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let a = arranged(subviews, width: proposal.width)
        return CGSize(width: a.width, height: a.beside ? max(a.words.height, a.control.height) : a.words.height + under + a.control.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let a = arranged(subviews, width: bounds.width)
        if a.beside {
            subviews[0].place(at: CGPoint(x: bounds.minX, y: bounds.midY), anchor: .leading, proposal: ProposedViewSize(a.words))
            subviews[1].place(at: CGPoint(x: bounds.maxX, y: bounds.midY), anchor: .trailing, proposal: ProposedViewSize(a.control))
        } else {
            subviews[0].place(at: CGPoint(x: bounds.minX, y: bounds.minY), proposal: ProposedViewSize(a.words))
            subviews[1].place(at: CGPoint(x: bounds.minX, y: bounds.minY + a.words.height + under), proposal: ProposedViewSize(a.control))
        }
    }
}

/// "Custom: these settings match no preset.", under the presets when none matches.
private struct CustomLine: View {
    var body: some View {
        let words = MessagePreset.custom
        let cut = words.firstIndex(of: ":").map { words.index(after: $0) } ?? words.startIndex
        HStack(spacing: LumiaTokens.space8) {
            Circle().fill(LumiaTokens.accent.color).frame(width: LumiaTokens.space6, height: LumiaTokens.space6)
                .accessibilityHidden(true)
            (Text(words[..<cut]).fontWeight(.semibold).foregroundStyle(LumiaTokens.ink.color) + Text(words[cut...]))
                .font(LumiaTokens.textMeta.font)
                .foregroundStyle(LumiaTokens.inkMuted.color)
        }
    }
}

// MARK: Choices

/// The compact segmented control: a track with a segment for each choice,
/// sized to its words (or `width` each), the chosen one raised. A group
/// named for what is chosen, each segment a button that says whether it is
/// the one selected.
private struct CompactSegments<ID: Hashable>: View {
    struct Segment: Identifiable {
        let id: ID
        let label: String
        /// What assistive technology says for it, when not its label ("18 points").
        var spoken: String?
    }

    @Environment(\.colorScheme) private var scheme
    let label: String
    let segments: [Segment]
    @Binding var selected: ID
    var width: CGFloat?

    /// A font size's segment: two digits, all the same width.
    static var numberWidth: CGFloat { LumiaTokens.space18 * 2 }
    /// A segment's height and the track's padding around it.
    private static var height: CGFloat { LumiaTokens.space28 + LumiaTokens.space2 }
    private static var inset: CGFloat { LumiaTokens.space6 / 2 }
    /// The raised segment's shadow: 1.5 soft, 1 down.
    private static var shadow: (radius: CGFloat, y: CGFloat) { (inset / 2, inset / 3) }

    /// The widest of the controls these labels would make, each segment
    /// sized to its words, as drawn.
    static func widest(_ controls: [[String]]) -> CGFloat {
        let font = UIFont.systemFont(ofSize: LumiaTokens.textLinkSm.size, weight: .semibold)
        return controls.map { labels in
            labels.map { ceil(($0 as NSString).size(withAttributes: [.font: font]).width) + 2 * LumiaTokens.space12 }.reduce(0, +)
                + CGFloat(max(0, labels.count - 1)) * LumiaTokens.space2 + 2 * inset
        }.max() ?? 0
    }

    var body: some View {
        HStack(spacing: LumiaTokens.space2) {
            ForEach(segments) { segment in
                let on = segment.id == selected
                Button { selected = segment.id } label: {
                    Text(segment.label)
                        .font(LumiaTokens.textLinkSm.font)
                        .monospacedDigit()
                        .lineLimit(1)
                        .foregroundStyle(on ? LumiaTokens.ink.color : LumiaTokens.inkMuted.color)
                        .padding(.horizontal, width == nil ? LumiaTokens.space12 : 0)
                        .frame(width: width, height: Self.height)
                        .background {
                            if on {
                                RoundedRectangle(cornerRadius: LumiaTokens.radius8)
                                    .fill(LumiaTokens.tabActive.color)
                                    // The tab's shadow: light only, where the raised segment is the track's own white.
                                    .shadow(color: scheme == .dark ? .clear : LumiaTokens.ink.color.opacity(0.12), radius: Self.shadow.radius, y: Self.shadow.y)
                            }
                        }
                        .contentShape(RoundedRectangle(cornerRadius: LumiaTokens.radius8))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(segment.spoken ?? segment.label)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .padding(Self.inset)
        .background(LumiaTokens.track.color, in: RoundedRectangle(cornerRadius: LumiaTokens.radius10))
        .fixedSize()
        .accessibilityElement(children: .contain)
        .accessibilityLabel(label)
    }
}

/// A switch's row: its name over what its state does, and the system's
/// switch, which assistive technology hears by the name, with what the
/// state does as its hint. A tap anywhere on the row turns it.
private struct SwitchRow: View {
    let setting: MessageSwitch
    @Binding var on: Bool

    var body: some View {
        let caption = setting.caption(on: on)
        HStack(spacing: LumiaTokens.space16) {
            RowWords(name: setting.name, caption: caption)
                .accessibilityHidden(true)
            Toggle(setting.name, isOn: $on)
                .labelsHidden()
                .tint(LumiaTokens.accent.color)
                .accessibilityHint(caption)
        }
        .padding(.vertical, LumiaTokens.space14)
        .padding(.horizontal, LumiaTokens.space16)
        .contentShape(Rectangle())
        .onTapGesture { on.toggle() }
    }
}

/// Three choices side by side, a group named for what is chosen.
struct ChoiceTiles<Content: View>: View {
    let label: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        HStack(alignment: .top, spacing: LumiaTokens.space12) { content() }
            .accessibilityElement(children: .contain)
            .accessibilityLabel(label)
    }
}

/// A picture over a name (and a caption): the chosen one ringed in the
/// accent 2 points out with a tick after its name, the others outlined.
struct ChoiceTile<Sketch: View>: View {
    let name: String
    var caption: String?
    let chosen: Bool
    let choose: () -> Void
    /// The picture's height where it is not a third of a phone's column (the two tiles of "Tag beside names").
    var height: CGFloat?
    @ViewBuilder let sketch: () -> Sketch

    /// The picture's height on a phone, where a tile is a third of the column.
    static var pictureHeight: CGFloat { LumiaTokens.space40 + LumiaTokens.space24 + LumiaTokens.space12 }
    /// The ring round the chosen one: 2 wide and 2 off the picture, so its
    /// middle runs 3 out from the picture's edge.
    private static var ring: CGFloat { LumiaTokens.space2 }
    private static var ringOut: CGFloat { ring + ring / 2 }
    /// The tick after the chosen one's name (11).
    private static var tick: CGFloat { LumiaTokens.textCount.size }

    var body: some View {
        Button(action: choose) {
            VStack(alignment: .leading, spacing: LumiaTokens.space10) {
                sketch()
                    .frame(maxWidth: .infinity, minHeight: height ?? Self.pictureHeight, maxHeight: height ?? Self.pictureHeight, alignment: .topLeading)
                    .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
                    .overlay {
                        if chosen {
                            RoundedRectangle(cornerRadius: LumiaTokens.radius12 + Self.ringOut)
                                .stroke(LumiaTokens.accent.color, lineWidth: Self.ring)
                                .padding(-Self.ringOut)
                        } else {
                            RoundedRectangle(cornerRadius: LumiaTokens.radius12).strokeBorder(LumiaTokens.lineStrong.color)
                        }
                    }
                VStack(alignment: .leading, spacing: LumiaTokens.space2) {
                    HStack(spacing: LumiaTokens.space6) {
                        Text(name).font(LumiaTokens.textMember.font)
                            .foregroundStyle(chosen ? LumiaTokens.ink.color : LumiaTokens.inkBody.color)
                            .lineLimit(1)
                            .minimumScaleFactor(0.8)
                        if chosen {
                            Image(systemName: "checkmark").font(.system(size: Self.tick, weight: .heavy)).foregroundStyle(LumiaTokens.accentInk.color)
                        }
                    }
                    if let caption {
                        Text(caption).font(LumiaTokens.textMeta.font).foregroundStyle(LumiaTokens.inkMuted.color)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(name)
        .accessibilityHint(caption ?? "")
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }
}

// MARK: Sketches

/// What the sketches are drawn at, from the tokens: a bar (the boards' 5
/// tall), the sample screenshot's heading (7) and its sign-in card (120
/// by 128).
private enum SketchMetrics {
    static let bar = LumiaTokens.space10 / 2
    static let heading = LumiaTokens.space14 / 2
    static let card = CGSize(width: LumiaTokens.space40 * 3, height: LumiaTokens.space40 * 3 + LumiaTokens.space8)
}

/// A bar of a sketch, `fraction` of the width it is given, with round ends.
private struct SketchBar: Shape {
    let fraction: CGFloat

    func path(in rect: CGRect) -> Path {
        Path(roundedRect: CGRect(x: rect.minX, y: rect.minY, width: rect.width * fraction, height: rect.height), cornerRadius: rect.height / 2)
    }
}

/// Bars of `color` under each other, each `fraction` of the width.
private struct SketchLines: View {
    let fractions: [CGFloat]
    let color: Color
    var height: CGFloat = SketchMetrics.bar

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space6) {
            ForEach(Array(fractions.enumerated()), id: \.offset) { _, f in
                SketchBar(fraction: f).fill(color).frame(height: height)
            }
        }
    }
}

/// The app's window in a theme, small: the rail, the sidebar's lines, and
/// the panel with an accent line over two of words. System is half light,
/// half dark. Drawn in that theme's colours whatever the phone is in.
private struct AppearanceSketch: View {
    let appearance: Appearance

    private static let rail = LumiaTokens.space10
    private static let sidebar = LumiaTokens.space28

    private func c(_ token: LumiaColor, dark: Bool) -> Color { (dark ? token.dark : token.light).color }

    private func sidebarLines(dark: Bool) -> some View {
        SketchLines(fractions: [0.8, 1, 0.64], color: c(LumiaTokens.railDivider, dark: dark))
            .padding(.vertical, LumiaTokens.space10)
            .padding(.horizontal, LumiaTokens.space6)
    }

    private func panel(dark: Bool) -> some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space6) {
            SketchBar(fraction: 0.5).fill(c(LumiaTokens.accent, dark: dark)).frame(height: SketchMetrics.bar)
            SketchLines(fractions: [0.9, 0.72], color: c(LumiaTokens.lineStrong, dark: dark))
        }
        .padding(LumiaTokens.space8)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(c(LumiaTokens.surface, dark: dark), in: RoundedRectangle(cornerRadius: LumiaTokens.radius7))
        .padding([.vertical, .trailing], LumiaTokens.space6)
    }

    var body: some View {
        switch appearance {
        case .light, .dark:
            let dark = appearance == .dark
            HStack(spacing: 0) {
                c(LumiaTokens.rail, dark: dark).frame(width: Self.rail)
                sidebarLines(dark: dark).frame(width: Self.sidebar, alignment: .top).frame(maxHeight: .infinity, alignment: .top)
                panel(dark: dark)
            }
            .background(c(LumiaTokens.bg, dark: dark))
        case .system:
            HStack(spacing: 0) {
                HStack(spacing: 0) {
                    c(LumiaTokens.rail, dark: false).frame(width: Self.rail)
                    sidebarLines(dark: false).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                }
                .background(c(LumiaTokens.bg, dark: false))
                panel(dark: true)
                    .padding(.leading, LumiaTokens.space6)
                    .background(c(LumiaTokens.bg, dark: true))
            }
        }
    }
}

/// A preset's layout, small: Comfortable two large avatars with two lines
/// each, Compact small avatars with the lines closer, Minimal five single
/// lines of a time, a name and words.
private struct PresetSketch: View {
    let preset: MessagePreset

    private var name: Color { LumiaTokens.borderActive.color }
    private var words: Color { LumiaTokens.borderForm.color }

    private func bar(_ fraction: CGFloat, _ color: Color) -> some View {
        SketchBar(fraction: fraction).fill(color).frame(height: SketchMetrics.bar)
    }

    /// A bar of a set width.
    private func stub(_ width: CGFloat, _ color: Color) -> some View {
        Capsule().fill(color).frame(width: width, height: SketchMetrics.bar)
    }

    /// Comfortable's: a large avatar beside a name and a line of words.
    private func block(_ tone: LumiaColor, name n: CGFloat, words w: CGFloat) -> some View {
        HStack(alignment: .top, spacing: LumiaTokens.space8) {
            Circle().fill(tone.color).frame(width: LumiaTokens.space18 + LumiaTokens.space2, height: LumiaTokens.space18 + LumiaTokens.space2)
            VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                bar(n, name)
                bar(w, words)
            }
            .padding(.top, LumiaTokens.space2)
        }
    }

    /// Compact's: a small avatar beside a name.
    private func head(_ tone: LumiaColor, name n: CGFloat) -> some View {
        HStack(spacing: LumiaTokens.space6) {
            Circle().fill(tone.color).frame(width: LumiaTokens.space12, height: LumiaTokens.space12)
            bar(n, name)
        }
    }

    /// Compact's line of words, under the name.
    private func under(_ w: CGFloat) -> some View {
        bar(w, words).padding(.leading, LumiaTokens.space12 + LumiaTokens.space6)
    }

    /// Minimal's line: a time, a name and words.
    private func line(name n: CGFloat, words w: CGFloat) -> some View {
        HStack(spacing: LumiaTokens.space4) {
            stub(LumiaTokens.space10, words)
            stub(n, name)
            bar(w, words)
        }
    }

    var body: some View {
        Group {
            switch preset {
            case .comfortable:
                VStack(alignment: .leading, spacing: LumiaTokens.space10) {
                    block(LumiaTokens.toneMint, name: 0.36, words: 0.88)
                    block(LumiaTokens.toneAmber, name: 0.3, words: 0.7)
                }
            case .compact:
                VStack(alignment: .leading, spacing: LumiaTokens.space4) {
                    head(LumiaTokens.toneMint, name: 0.3)
                    under(0.8)
                    head(LumiaTokens.toneAmber, name: 0.26)
                    under(0.64)
                    under(0.72)
                }
            case .minimal:
                VStack(alignment: .leading, spacing: LumiaTokens.space6) {
                    line(name: LumiaTokens.space18, words: 0.5)
                    line(name: LumiaTokens.space18, words: 0.38)
                    line(name: LumiaTokens.space14, words: 0.6)
                    line(name: LumiaTokens.space18, words: 0.3)
                    line(name: LumiaTokens.space14, words: 0.46)
                }
            }
        }
        .padding(LumiaTokens.space10)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(LumiaTokens.track.color)
    }
}

// MARK: Preview

/// "Preview", then "# design-review" and its four sample messages drawn by
/// the conversation's own rows (`LumiaMessageRow`, with its avatars, names,
/// times, words, link and reaction chips) under the settings as they are
/// now, then the line saying the choices are this phone's. Nothing in it
/// is pressable or focusable, and assistive technology hears it as one
/// picture.
private struct MessagesPreview: View {
    let measures: MessageMeasures
    let images: MediaDisplay
    let embeds: MediaDisplay

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space10) {
            Text(PreferenceWords.preview)
                .textCase(.uppercase)
                .font(LumiaTokens.textCount.font)
                .tracking(LumiaTokens.textCount.size * 0.06)
                .foregroundStyle(LumiaTokens.inkMuted.color)
                .accessibilityAddTraits(.isHeader)
            box
            Text(PreferenceWords.kept)
                .font(LumiaTokens.textMeta.font)
                .foregroundStyle(LumiaTokens.inkMuted.color)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private var box: some View {
        messages
            .allowsHitTesting(false)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(PreferenceWords.previewLabel)
            .accessibilityAddTraits(.isImage)
    }

    private var messages: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: LumiaTokens.space8) {
                Text("#").font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.inkMuted.color)
                Text(PreferencesPreview.channel).font(LumiaTokens.textAuthor.font).foregroundStyle(LumiaTokens.ink.color)
            }
            .padding(.horizontal, LumiaTokens.space16)
            .padding(.vertical, LumiaTokens.space12)
            .frame(maxWidth: .infinity, alignment: .leading)
            LumiaTokens.line.color.frame(height: 1)
            VStack(alignment: .leading, spacing: 0) {
                ForEach(PreferencesPreview.messages) { m in
                    LumiaMessageRow(author: m.author, tone: .of(m.author), time: m.time, text: m.text, place: m.place, measures: measures, extras: extras(m))
                }
            }
            // The conversation list's own room around its rows.
            .padding(.vertical, LumiaTokens.space8)
        }
        .background(LumiaTokens.surface.color)
        .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius16))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius16).strokeBorder(LumiaTokens.lineStrong.color))
    }

    /// What the conversation adds to a row, of what a sample has: its
    /// reactions (drawn or not by the measures) and what it carries.
    private func extras(_ m: PreferencesPreview.Message) -> MessageExtras {
        var x = MessageExtras()
        x.reactions = m.reactions
        // As the conversation does, for a message that carries anything, even when nothing of it is drawn.
        if m.picture || m.card {
            x.media = AnyView(PreviewMedia(pieces: PreferencesPreview.pieces(m, images: images, embeds: embeds)))
        }
        return x
    }
}

/// What a sample carries, laid out as `MessageMedia` lays a message's out:
/// the picture at its shape (`PictureGrid`), or, with Images as links, the
/// conversation's own file row (`FileRow`); then the conversation's own
/// card (`LinkCardView`, with no session: its card has no picture to fetch
/// and its link no embed). `MessageMedia` itself needs a session.
private struct PreviewMedia: View {
    let pieces: [PreferencesPreview.Piece]

    var body: some View {
        VStack(alignment: .leading, spacing: LumiaTokens.space6) {
            ForEach(pieces, id: \.self) { piece in
                switch piece {
                case .picture:
                    PictureGrid(sizes: [(PreferencesPreview.pictureWidth, PreferencesPreview.pictureHeight)], gap: LumiaTokens.space2) {
                        SampleScreenshot()
                    }
                case .file:
                    FileRow(attachment: PreferencesPreview.file, asFile: true) {}
                case .card:
                    LinkCardView(card: PreferencesPreview.card, session: nil)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Tomás's screenshot, drawn: a picture in a message is the attachment's
/// tile (`AttachmentTile`), which fetches and decodes it through the
/// session, so this draws one that looks the same, the tile's corners and
/// edge over a sketch of a sign-in card on the blue tone.
private struct SampleScreenshot: View {
    private static let card = SketchMetrics.card
    private static let field = LumiaTokens.space18

    var body: some View {
        ZStack(alignment: .bottom) {
            LumiaTokens.toneBlue.color
            VStack(alignment: .leading, spacing: LumiaTokens.space8) {
                SketchBar(fraction: 0.6).fill(LumiaTokens.borderActive.color).frame(height: SketchMetrics.heading)
                RoundedRectangle(cornerRadius: LumiaTokens.radius6).strokeBorder(LumiaTokens.lineStrong.color).frame(height: Self.field)
                RoundedRectangle(cornerRadius: LumiaTokens.radius6).strokeBorder(LumiaTokens.lineStrong.color).frame(height: Self.field)
                RoundedRectangle(cornerRadius: LumiaTokens.radius6).fill(LumiaTokens.accent.color).frame(height: Self.field)
            }
            .padding(.horizontal, LumiaTokens.space12)
            .padding(.top, LumiaTokens.space14)
            .frame(width: Self.card.width, height: Self.card.height, alignment: .top)
            .background(LumiaTokens.surface.color, in: UnevenRoundedRectangle(topLeadingRadius: LumiaTokens.radius12, topTrailingRadius: LumiaTokens.radius12))
        }
        .clipShape(RoundedRectangle(cornerRadius: LumiaTokens.radius12))
        .overlay(RoundedRectangle(cornerRadius: LumiaTokens.radius12).stroke(LumiaTokens.line.color))
    }
}
