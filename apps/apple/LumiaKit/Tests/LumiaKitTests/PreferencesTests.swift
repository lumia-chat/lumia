// Preferences as values: what an appearance resolves to, what the presets
// write and which one a set of values is, what each spacing and avatar
// size measures, whether reactions are drawn, and the choices read and
// written with their defaults.
// The plan's "Display settings", the same on every client.
import Foundation
@testable import LumiaKit
import SwiftUI
import XCTest

@MainActor
final class PreferencesTests: XCTestCase {
    /// A store of its own for each test, gone when the test is.
    private func scratch() -> UserDefaults {
        let name = "lumia-preferences-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        addTeardownBlock { defaults.removePersistentDomain(forName: name) }
        return defaults
    }

    // MARK: Appearance

    func testLightAndDarkAreTheChoiceMadeAndSystemIsWhicheverThePhoneIsIn() {
        for systemDark in [false, true] {
            XCTAssertFalse(Appearance.light.dark(systemDark: systemDark))
            XCTAssertTrue(Appearance.dark.dark(systemDark: systemDark))
            XCTAssertEqual(Appearance.system.dark(systemDark: systemDark), systemDark)
        }
        // What the app asks of the system: nothing for System, so the phone is followed as it changes.
        XCTAssertEqual(Appearance.light.colorScheme, .light)
        XCTAssertEqual(Appearance.dark.colorScheme, .dark)
        XCTAssertNil(Appearance.system.colorScheme)
    }

    func testTheChoicesAreInThePagesOrderUnderItsWords() {
        XCTAssertEqual(Appearance.allCases.map(\.label), ["Light", "Dark", "System"])
        XCTAssertEqual(MessagePreset.allCases.map(\.label), ["Comfortable", "Compact", "Minimal"])
        XCTAssertEqual(MessageLayout.allCases.map(\.label), ["Grouped", "One line"])
        XCTAssertEqual(MessageSpacing.allCases.map(\.label), ["Roomy", "Cozy", "Tight"])
        XCTAssertEqual(AvatarSize.allCases.map(\.label), ["Large", "Small", "Hidden"])
        XCTAssertEqual(FontSize.choices.map(\.label), ["13", "14", "15", "16", "18", "20"])
        XCTAssertEqual(ReactionVisibility.allCases.map(\.label), ["Shown", "Hidden"])
        XCTAssertEqual(MessagePreset.custom, "Custom: these settings match no preset.")
        // The stored words, the same on every client.
        XCTAssertEqual(Appearance.allCases.map(\.rawValue), ["light", "dark", "system"])
        XCTAssertEqual(MessageLayout.allCases.map(\.rawValue), ["grouped", "one_line"])
        XCTAssertEqual(MessageSpacing.allCases.map(\.rawValue), ["roomy", "cozy", "tight"])
        XCTAssertEqual(AvatarSize.allCases.map(\.rawValue), ["large", "small", "hidden"])
        XCTAssertEqual(MediaDisplay.allCases.map(\.rawValue), ["shown", "link"])
        XCTAssertEqual(ReactionVisibility.allCases.map(\.rawValue), ["shown", "hidden"])
        XCTAssertEqual(FontSize.choices.map(\.points), [13, 14, 15, 16, 18, 20])
        XCTAssertEqual([Preferences.layoutKey, Preferences.spacingKey, Preferences.avatarsKey, Preferences.fontSizeKey, Preferences.imagesKey, Preferences.embedsKey, Preferences.reactionsKey], ["layout", "spacing", "avatars", "font_size", "images", "embeds", "reactions"])
    }

    // MARK: Presets

    func testEachPresetIsItsValues() {
        XCTAssertEqual(MessagePreset.comfortable.display, MessageDisplay(layout: .grouped, spacing: .roomy, avatars: .large, images: .shown, embeds: .shown, reactions: .shown))
        XCTAssertEqual(MessagePreset.compact.display, MessageDisplay(layout: .grouped, spacing: .cozy, avatars: .small, images: .shown, embeds: .shown, reactions: .shown))
        XCTAssertEqual(MessagePreset.minimal.display, MessageDisplay(layout: .oneLine, spacing: .tight, avatars: .hidden, images: .link, embeds: .link, reactions: .hidden))
        // Each setting's own default is Comfortable's value.
        XCTAssertEqual(MessageDisplay.standard, MessagePreset.comfortable.display)
        for preset in MessagePreset.allCases { XCTAssertEqual(MessagePreset.matching(preset.display), preset) }
    }

    func testAnyOneSettingChangedFromAPresetIsCustom() {
        for preset in MessagePreset.allCases {
            let d = preset.display
            var changed: [MessageDisplay] = []
            for layout in MessageLayout.allCases where layout != d.layout { var x = d; x.layout = layout; changed.append(x) }
            for spacing in MessageSpacing.allCases where spacing != d.spacing { var x = d; x.spacing = spacing; changed.append(x) }
            for media in MediaDisplay.allCases where media != d.images { var x = d; x.images = media; changed.append(x) }
            for media in MediaDisplay.allCases where media != d.embeds { var x = d; x.embeds = media; changed.append(x) }
            for reactions in ReactionVisibility.allCases where reactions != d.reactions { var x = d; x.reactions = reactions; changed.append(x) }
            if d.layout == .grouped {
                for avatars in AvatarSize.allCases where avatars != d.avatars { var x = d; x.avatars = avatars; changed.append(x) }
            }
            for x in changed { XCTAssertNil(MessagePreset.matching(x), "\(x) is not \(preset)") }
        }
    }

    func testOneLineDrawsNoAvatarSoItsAvatarsAreNotCompared() {
        for avatars in AvatarSize.allCases {
            var d = MessagePreset.minimal.display
            d.avatars = avatars
            XCTAssertEqual(MessagePreset.matching(d), .minimal)
        }
        // In grouped they are: Comfortable with small avatars is nobody's.
        var d = MessagePreset.comfortable.display
        d.avatars = .small
        XCTAssertNil(MessagePreset.matching(d))
    }

    func testChoosingAPresetWritesItsValuesAndNotTheFontSize() {
        let defaults = scratch()
        let preferences = Preferences(defaults: defaults, environment: [:])
        preferences.fontSize = FontSize(18)!
        preferences.preset = .minimal
        XCTAssertEqual(preferences.display, MessagePreset.minimal.display)
        XCTAssertEqual(preferences.preset, .minimal)
        XCTAssertEqual(preferences.fontSize.points, 18)
        // Each value is stored on its own; the preset is not.
        XCTAssertEqual(defaults.string(forKey: Preferences.layoutKey), "one_line")
        XCTAssertEqual(defaults.string(forKey: Preferences.spacingKey), "tight")
        XCTAssertEqual(defaults.string(forKey: Preferences.avatarsKey), "hidden")
        XCTAssertEqual(defaults.string(forKey: Preferences.imagesKey), "link")
        XCTAssertEqual(defaults.string(forKey: Preferences.embedsKey), "link")
        XCTAssertEqual(defaults.string(forKey: Preferences.reactionsKey), "hidden")
        XCTAssertEqual(defaults.integer(forKey: Preferences.fontSizeKey), 18)
        XCTAssertFalse(defaults.dictionaryRepresentation().keys.contains("preset"))
        // A tweak is Custom; choosing nothing changes nothing.
        preferences.spacing = .roomy
        XCTAssertNil(preferences.preset)
        preferences.preset = nil
        XCTAssertEqual(preferences.spacing, .roomy)
        XCTAssertEqual(preferences.layout, .oneLine)
        // And a preset again writes every one of its values back.
        preferences.preset = .compact
        XCTAssertEqual(preferences.display, MessagePreset.compact.display)
        XCTAssertEqual(defaults.string(forKey: Preferences.imagesKey), "shown")
        XCTAssertEqual(defaults.string(forKey: Preferences.reactionsKey), "shown")
    }

    // MARK: The page

    func testThePageSaysWhatEveryClientSaysInAPhonesWords() {
        XCTAssertEqual(PreferenceWords.topic, "How Lumia looks on this phone")
        XCTAssertEqual(PreferenceWords.appearance, "Light, dark, or whichever this phone is in.")
        XCTAssertEqual(PreferenceWords.messages, "Start from a preset, then change anything below.")
        XCTAssertEqual(PreferenceWords.previewLabel, "Preview of messages with these settings")
        XCTAssertEqual(PreferenceWords.kept, "Kept on this phone only. Your other devices and your account are not changed.")
        XCTAssertEqual(MessagePreset.allCases.map(\.caption), ["Large avatars, room to breathe", "Small avatars, more on screen", "A line each, media as links"])
        XCTAssertEqual(MessageSetting.allCases.map(\.name), ["Layout", "Spacing", "Avatars", "Font size"])
        XCTAssertEqual(MessageSetting.allCases.map(\.caption), ["One person’s messages as a block, or a line each.", "Room between messages.", "Beside each block of messages.", "Messages and names, in points."])
        XCTAssertEqual(MessageSwitch.allCases.map(\.name), ["Images", "Embeds", "Reactions"])
    }

    func testAvatarsAreARowOnlyWhileGrouped() {
        XCTAssertEqual(MessageSetting.shown(.grouped), [.layout, .spacing, .avatars, .fontSize])
        XCTAssertEqual(MessageSetting.shown(.oneLine), [.layout, .spacing, .fontSize])
        // Minimal is one line: choosing it takes the row away.
        let preferences = Preferences(defaults: scratch(), environment: [:])
        XCTAssertTrue(MessageSetting.shown(preferences.layout).contains(.avatars))
        preferences.preset = .minimal
        XCTAssertFalse(MessageSetting.shown(preferences.layout).contains(.avatars))
    }

    func testEachSwitchSaysWhatItsStateDoes() {
        XCTAssertEqual(MessageSwitch.images.caption(on: true), "Shown in their messages.")
        XCTAssertEqual(MessageSwitch.images.caption(on: false), "As links: each one a file row that opens the viewer.")
        XCTAssertEqual(MessageSwitch.embeds.caption(on: true), "A card under each link.")
        XCTAssertEqual(MessageSwitch.embeds.caption(on: false), "As links: no cards.")
        XCTAssertEqual(MessageSwitch.reactions.caption(on: true), "Shown under each message.")
        // A phone adds one from a message's menu, not a toolbar.
        XCTAssertEqual(MessageSwitch.reactions.caption(on: false), "Hidden. You can still add one from a message’s menu.")
    }

    func testASwitchIsItsSettingShownAndStoresTheSameValues() {
        let defaults = scratch()
        let preferences = Preferences(defaults: defaults, environment: [:])
        for setting in MessageSwitch.allCases { XCTAssertTrue(preferences.isOn(setting), "\(setting) is on on a new phone") }
        preferences.set(.images, on: false)
        XCTAssertEqual(preferences.images, .link)
        XCTAssertEqual(defaults.string(forKey: Preferences.imagesKey), "link")
        preferences.set(.embeds, on: false)
        XCTAssertEqual(defaults.string(forKey: Preferences.embedsKey), "link")
        preferences.set(.reactions, on: false)
        XCTAssertEqual(preferences.reactions, .hidden)
        XCTAssertEqual(defaults.string(forKey: Preferences.reactionsKey), "hidden")
        XCTAssertEqual(MessageSwitch.allCases.map(preferences.isOn), [false, false, false])
        // A switch turned from Comfortable is Custom, as any other setting is.
        let again = Preferences(defaults: scratch(), environment: [:])
        again.set(.embeds, on: false)
        XCTAssertNil(again.preset)
        again.set(.embeds, on: true)
        XCTAssertEqual(again.preset, .comfortable)
        // Minimal turns all three off; on again, they are Shown.
        again.preset = .minimal
        XCTAssertEqual(MessageSwitch.allCases.map(again.isOn), [false, false, false])
        again.set(.reactions, on: true)
        XCTAssertEqual(again.reactions, .shown)
        XCTAssertNil(again.preset)
    }

    func testThePreviewIsFourMessagesDrawnAsTheSettingsSay() {
        let m = PreferencesPreview.messages
        XCTAssertEqual(PreferencesPreview.channel, "design-review")
        XCTAssertEqual(m.map(\.author), ["Priya Nair", "Priya Nair", "Tomás Reyes", "Mei Chen"])
        XCTAssertEqual(m.map(\.time), ["10:42", "10:42", "10:44", "10:51"])
        // Priya's two are one block.
        XCTAssertEqual(m.map(\.place.opens), [true, false, true, true])
        XCTAssertEqual(m.map(\.place.closes), [false, true, true, true])
        // Tomás's reactions, the 🎉 this account's.
        XCTAssertEqual(m[2].reactions.map(\.emoji), ["👀", "🎉"])
        XCTAssertEqual(m[2].reactions.map(\.count), [2, 3])
        XCTAssertEqual(m[2].reactions.map(\.me), [false, true])
        // Mei's address is a link, as the core reads the body.
        XCTAssertTrue(m[3].text.hasSuffix("[lumia.chat/blog/0-13](https://lumia.chat/blog/0-13)"))
        // What each carries, by Images and Embeds.
        XCTAssertEqual(PreferencesPreview.pieces(m[2], images: .shown, embeds: .shown), [.picture])
        XCTAssertEqual(PreferencesPreview.pieces(m[2], images: .link, embeds: .shown), [.file])
        XCTAssertEqual(PreferencesPreview.pieces(m[3], images: .shown, embeds: .shown), [.card])
        XCTAssertEqual(PreferencesPreview.pieces(m[3], images: .shown, embeds: .link), [])
        XCTAssertEqual(PreferencesPreview.pieces(m[0], images: .shown, embeds: .shown), [])
        // The file row's words, and a card with no picture, so nothing is fetched for either.
        XCTAssertEqual(PreferencesPreview.file.name, "onboarding-first-screen.png")
        XCTAssertEqual(MediaWords.detail(kind: .image, sizeBytes: PreferencesPreview.file.sizeBytes, durationMs: 0), MediaWords.size(412_000))
        XCTAssertEqual([PreferencesPreview.card.host, PreferencesPreview.card.title], ["lumia.chat", "What’s new in Lumia 0.13"])
        XCTAssertTrue(PreferencesPreview.card.imageUrl.isEmpty)
        XCTAssertNil(PreferencesPreview.card.image)
    }

    // MARK: Reactions

    func testReactionsAreShownButInMinimal() {
        XCTAssertEqual(ReactionVisibility.standard, .shown)
        XCTAssertEqual(MessageDisplay.standard.reactions, .shown)
        XCTAssertEqual(MessagePreset.allCases.map(\.display.reactions), [.shown, .shown, .hidden])
        // The rows are drawn by it: Minimal's draw no chips, the others' do.
        XCTAssertEqual(MessageMeasures.standard.reactions, .shown)
        XCTAssertEqual(MessagePreset.allCases.map { MessageMeasures.of($0.display).reactions }, [.shown, .shown, .hidden])
    }

    func testReactionsAreComparedLikeEveryOtherValue() {
        // Minimal with its reactions shown is nobody's, and so is Comfortable with them hidden.
        var minimal = MessagePreset.minimal.display
        minimal.reactions = .shown
        XCTAssertNil(MessagePreset.matching(minimal))
        var comfortable = MessagePreset.comfortable.display
        comfortable.reactions = .hidden
        XCTAssertNil(MessagePreset.matching(comfortable))
        // Chosen on the page after Minimal, they make it Custom; hidden again, it is Minimal.
        let preferences = Preferences(defaults: scratch(), environment: [:])
        preferences.preset = .minimal
        XCTAssertEqual(preferences.reactions, .hidden)
        XCTAssertEqual(preferences.measures().reactions, .hidden)
        preferences.reactions = .shown
        XCTAssertNil(preferences.preset)
        XCTAssertEqual(preferences.measures().reactions, .shown)
        preferences.reactions = .hidden
        XCTAssertEqual(preferences.preset, .minimal)
    }

    func testReactionsMissingOrNotUnderstoodAreShown() {
        let defaults = scratch()
        XCTAssertEqual(Preferences(defaults: defaults, environment: [:]).reactions, .shown)
        for stored: Any in ["Hidden", "none", "", "link", true, 0] {
            defaults.set(stored, forKey: Preferences.reactionsKey)
            XCTAssertEqual(Preferences(defaults: defaults, environment: [:]).reactions, .shown, "\(stored)")
        }
        defaults.set("hidden", forKey: Preferences.reactionsKey)
        XCTAssertEqual(Preferences(defaults: defaults, environment: [:]).reactions, .hidden)
        XCTAssertEqual(ReactionVisibility(stored: nil), .shown)
    }

    // MARK: Measures

    func testRoomyWithLargeAvatarsIsTheRowAsItWasDrawnBeforeThereWereSettings() {
        let m = MessageMeasures.of(spacing: .roomy, avatars: .large)
        XCTAssertEqual(m, MessageMeasures(avatar: 40, edge: 8, blockGap: 8, beside: 14, underName: 4, divider: 8, line: 4))
        XCTAssertEqual(m, MessageMeasures.standard)
        XCTAssertEqual(m.avatar, LumiaTokens.avatarLg)
        // Two messages of a block: 4 from each, the leading and the paragraph's room.
        XCTAssertEqual(m.within, 4)
        XCTAssertEqual(MessageMeasures.leading, 4)
        XCTAssertEqual(m.gutter, 54)
    }

    func testCozyIsHalfTheRoomAndTightLessOverAndBetweenBlocks() {
        let (roomy, cozy, tight) = (MessageMeasures.of(spacing: .roomy, avatars: .large), MessageMeasures.of(spacing: .cozy, avatars: .large), MessageMeasures.of(spacing: .tight, avatars: .large))
        XCTAssertEqual(cozy.blockGap, roomy.blockGap / 2, "between blocks")
        XCTAssertEqual(cozy.edge, roomy.edge / 2, "over a block's first message and under its last")
        XCTAssertEqual(cozy.underName, roomy.underName / 2, "between a block's messages, and under the name")
        XCTAssertEqual(cozy.divider, roomy.divider / 2, "around a day's divider")
        XCTAssertEqual([roomy.beside, cozy.beside], [14, 10])
        // The leading between a message's own lines is not the spacing's, so
        // two messages are that and the halved room apart: 6 where it was 8.
        XCTAssertEqual(cozy.within * 2, 6)
        XCTAssertEqual(roomy.within * 2, 8)
        // Tight is cozy with the edge and the block gap 2.
        XCTAssertEqual([tight.edge, tight.blockGap], [2, 2])
        XCTAssertEqual([tight.divider, tight.beside, tight.underName], [cozy.divider, cozy.beside, cozy.underName])
    }

    func testAvatarsAreTheirOwnSettingAndHiddenLeavesNoGutter() {
        for spacing in MessageSpacing.allCases {
            XCTAssertEqual(MessageMeasures.of(spacing: spacing, avatars: .large).avatar, LumiaTokens.avatarLg)
            XCTAssertEqual(MessageMeasures.of(spacing: spacing, avatars: .small).avatar, LumiaTokens.avatarSm)
            let hidden = MessageMeasures.of(spacing: spacing, avatars: .hidden)
            XCTAssertEqual(hidden.avatar, 0)
            XCTAssertEqual(hidden.gutter, 0, "the name starts where the avatar did")
        }
        // Compact's measures are what they were: the small avatar, half the room.
        XCTAssertEqual(MessageMeasures.of(MessagePreset.compact.display), MessageMeasures(avatar: 28, edge: 4, blockGap: 4, beside: 10, underName: 2, divider: 4, line: 2))
    }

    func testOneLineHasNoAvatarAndItsRoomIsBetweenLines() {
        let lines = MessageSpacing.allCases.map { MessageMeasures.of(spacing: $0, avatars: .large, layout: .oneLine) }
        XCTAssertEqual(lines.map(\.line), [4, 2, 0])
        XCTAssertEqual(lines.map(\.divider), [8, 4, 4])
        XCTAssertEqual(lines.map(\.avatar), [0, 0, 0])
        XCTAssertEqual(lines.map(\.gutter), [0, 0, 0])
        // A row's room is half the leading and half the room between lines,
        // so tight lines are as far apart as one message's own.
        XCTAssertEqual(lines.map { $0.within * 2 }, [8, 6, 4])
        XCTAssertEqual(MessageMeasures.of(MessagePreset.minimal.display).layout, .oneLine)
    }

    func testTheFontSizeScalesTheTextAndTheLeadingOverTheSystemsSize() {
        XCTAssertEqual(FontSize.standard.scale, 1)
        XCTAssertEqual(FontSize(20)!.scale, 20.0 / 15.0, accuracy: 0.0001)
        XCTAssertEqual(FontSize(13)!.scale, 13.0 / 15.0, accuracy: 0.0001)
        // On top of the system's text size, not in place of it.
        let m = MessageMeasures.of(.standard, fontSize: FontSize(18)!, system: 1.5)
        XCTAssertEqual(m.text, 18.0 / 15.0 * 1.5, accuracy: 0.0001)
        XCTAssertEqual(m.lineSpacing, 4 * 1.8, accuracy: 0.0001)
        // Avatars and room do not grow.
        let base = MessageMeasures.standard
        XCTAssertEqual([m.avatar, m.edge, m.blockGap, m.beside, m.underName, m.divider], [base.avatar, base.edge, base.blockGap, base.beside, base.underName, base.divider])
        XCTAssertEqual(LumiaTokens.textMeta.font(scale: 1), LumiaTokens.textMeta.font)
    }

    // MARK: Stored

    func testAPhoneThatHasNeverChosenFollowsTheSystemAndIsComfortable() {
        let preferences = Preferences(defaults: scratch(), environment: [:])
        XCTAssertEqual(preferences.appearance, .system)
        XCTAssertEqual(preferences.preset, .comfortable)
        XCTAssertEqual(preferences.display, .standard)
        XCTAssertEqual(preferences.fontSize, .standard)
        XCTAssertEqual(preferences.measures(), .standard)
    }

    func testEachChoiceIsKeptForTheNextLaunch() {
        let defaults = scratch()
        let preferences = Preferences(defaults: defaults, environment: [:])
        preferences.appearance = .dark
        preferences.layout = .oneLine
        preferences.spacing = .cozy
        preferences.avatars = .small
        preferences.fontSize = FontSize(13)!
        preferences.images = .link
        preferences.embeds = .shown
        preferences.reactions = .hidden
        XCTAssertEqual(defaults.string(forKey: Preferences.appearanceKey), "dark")
        // A second launch over the same store has them, before anything is asked.
        let again = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(again.appearance, .dark)
        XCTAssertEqual(again.display, MessageDisplay(layout: .oneLine, spacing: .cozy, avatars: .small, images: .link, embeds: .shown, reactions: .hidden))
        XCTAssertEqual(again.fontSize.points, 13)
        XCTAssertNil(again.preset)
        // And back: System is stored as a choice, not as nothing.
        again.appearance = .system
        again.preset = .comfortable
        again.fontSize = .standard
        XCTAssertEqual(defaults.string(forKey: Preferences.appearanceKey), "system")
        let third = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(third.appearance, .system)
        XCTAssertEqual(third.preset, .comfortable)
        XCTAssertEqual(defaults.integer(forKey: Preferences.fontSizeKey), 15)
    }

    func testWhatIsStoredAndNotUnderstoodReadsAsEachDefault() {
        let defaults = scratch()
        defaults.set("sepia", forKey: Preferences.appearanceKey)
        defaults.set("One line", forKey: Preferences.layoutKey)
        defaults.set("Tight", forKey: Preferences.spacingKey)
        defaults.set("tiny", forKey: Preferences.avatarsKey)
        defaults.set(17, forKey: Preferences.fontSizeKey)
        defaults.set("hidden", forKey: Preferences.imagesKey)
        defaults.set(true, forKey: Preferences.embedsKey)
        var preferences = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(preferences.appearance, .system)
        XCTAssertEqual(preferences.display, .standard)
        XCTAssertEqual(preferences.fontSize, .standard)
        // Not the kind of value it should be, or a size that is not whole.
        defaults.set(2, forKey: Preferences.layoutKey)
        defaults.set(["tight"], forKey: Preferences.spacingKey)
        defaults.set("18", forKey: Preferences.fontSizeKey)
        preferences = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(preferences.display, .standard)
        XCTAssertEqual(preferences.fontSize, .standard)
        defaults.set(15.5, forKey: Preferences.fontSizeKey)
        XCTAssertEqual(Preferences(defaults: defaults, environment: [:]).fontSize, .standard)
        // Each is read on its own: one not understood leaves the others as stored.
        defaults.set("tight", forKey: Preferences.spacingKey)
        defaults.set(20, forKey: Preferences.fontSizeKey)
        preferences = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(preferences.spacing, .tight)
        XCTAssertEqual(preferences.layout, .grouped)
        XCTAssertEqual(preferences.fontSize.points, 20)
        XCTAssertEqual(FontSize(stored: nil), .standard)
        XCTAssertNil(FontSize(17))
        XCTAssertEqual(MessageLayout(stored: ""), .grouped)
    }

    func testTheDensityKeptBeforeIsNotRead() {
        let defaults = scratch()
        defaults.set("compact", forKey: "density")
        let preferences = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(preferences.preset, .comfortable)
    }

    func testAChoiceIsReadByWhatObservesIt() {
        let preferences = Preferences(defaults: scratch(), environment: [:])
        let changed = expectation(description: "the preset's readers are told")
        withObservationTracking { _ = preferences.preset } onChange: { changed.fulfill() }
        preferences.spacing = .tight
        wait(for: [changed], timeout: 1)
    }

    // The smoke runs' launches (LUMIA_DEV_APPEARANCE, LUMIA_DEV_PRESET, LUMIA_DEV_FONT_SIZE).
    func testALaunchCanNameTheChoicesWithoutStoringThem() {
        let defaults = scratch()
        defaults.set("light", forKey: Preferences.appearanceKey)
        defaults.set(14, forKey: Preferences.fontSizeKey)
        let named = Preferences(defaults: defaults, environment: ["LUMIA_DEV_APPEARANCE": "dark", "LUMIA_DEV_PRESET": "minimal", "LUMIA_DEV_FONT_SIZE": "20"])
        XCTAssertEqual(named.appearance, .dark)
        XCTAssertEqual(named.preset, .minimal)
        XCTAssertEqual(named.reactions, .hidden)
        XCTAssertEqual(named.fontSize.points, 20)
        XCTAssertEqual(defaults.string(forKey: Preferences.appearanceKey), "light")
        XCTAssertNil(defaults.string(forKey: Preferences.layoutKey))
        XCTAssertNil(defaults.string(forKey: Preferences.reactionsKey))
        XCTAssertEqual(defaults.integer(forKey: Preferences.fontSizeKey), 14)
        // The next launch without them reads what was there before.
        let next = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(next.appearance, .light)
        XCTAssertEqual(next.preset, .comfortable)
        XCTAssertEqual(next.fontSize.points, 14)
        // A word that is not a choice, or none, names nothing.
        let unnamed = Preferences(defaults: defaults, environment: ["LUMIA_DEV_APPEARANCE": "", "LUMIA_DEV_PRESET": "custom", "LUMIA_DEV_FONT_SIZE": "17"])
        XCTAssertEqual(unnamed.appearance, .light)
        XCTAssertEqual(unnamed.preset, .comfortable)
        XCTAssertEqual(unnamed.fontSize.points, 14)
    }

    // The choices are this phone's and not the account's: ending the
    // session (signing out, or forgetting the instance) leaves them.
    func testSigningOutLeavesTheChoices() async {
        let keys = [Preferences.appearanceKey, Preferences.layoutKey, Preferences.spacingKey, Preferences.avatarsKey, Preferences.fontSizeKey, Preferences.imagesKey, Preferences.embedsKey, Preferences.reactionsKey]
        let before = keys.map { UserDefaults.standard.object(forKey: $0) }
        defer { for (key, value) in zip(keys, before) { UserDefaults.standard.set(value, forKey: key) } }
        let preferences = Preferences(defaults: .standard, environment: [:])
        preferences.appearance = .dark
        preferences.preset = .minimal
        preferences.fontSize = FontSize(16)!
        let store = AppStore()
        await store.signOut()
        await store.forgetInstance()
        XCTAssertEqual(store.route, .signIn)
        let after = Preferences(defaults: .standard, environment: [:])
        XCTAssertEqual(after.appearance, .dark)
        XCTAssertEqual(after.preset, .minimal)
        XCTAssertEqual(after.fontSize.points, 16)
    }
}
