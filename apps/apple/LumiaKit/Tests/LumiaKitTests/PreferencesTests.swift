// Preferences as values: what an appearance resolves to, what each
// density measures, and the choices read and written with their defaults.
// The desktop shell's rules (apps/desktop/tests/preferences.rs).
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

    func testTheChoicesAreInTheDesktopsOrderUnderItsWords() {
        XCTAssertEqual(Appearance.allCases.map(\.label), ["Light", "Dark", "System"])
        XCTAssertEqual(Density.allCases.map(\.label), ["Comfortable", "Compact"])
        // The stored words, the same as the desktop's settings.json.
        XCTAssertEqual(Appearance.allCases.map(\.rawValue), ["light", "dark", "system"])
        XCTAssertEqual(Density.allCases.map(\.rawValue), ["comfortable", "compact"])
    }

    // MARK: Measures

    func testComfortableIsTheRowAsItWasDrawnBeforeThereWasAChoice() {
        let m = MessageMeasures.of(.comfortable)
        XCTAssertEqual(m, MessageMeasures(avatar: 40, edge: 8, blockGap: 8, beside: 14, underName: 4, divider: 8))
        XCTAssertEqual(m.avatar, LumiaTokens.avatarLg)
        // Two messages of a block: 4 from each, the leading and the paragraph's room.
        XCTAssertEqual(m.within, 4)
        XCTAssertEqual(MessageMeasures.leading, 4)
    }

    func testCompactIsTheSmallAvatarAndHalfTheRoom() {
        let (roomy, tight) = (MessageMeasures.of(.comfortable), MessageMeasures.of(.compact))
        XCTAssertEqual(tight.avatar, LumiaTokens.avatarSm)
        XCTAssertLessThan(tight.avatar, roomy.avatar)
        XCTAssertEqual(tight.blockGap, roomy.blockGap / 2, "between blocks")
        XCTAssertEqual(tight.edge, roomy.edge / 2, "over a block's first message and under its last")
        XCTAssertEqual(tight.underName, roomy.underName / 2, "between a block's messages, and under the name")
        XCTAssertEqual(tight.divider, roomy.divider / 2, "around a day's divider")
        XCTAssertLessThan(tight.beside, roomy.beside)
        // The leading between a message's own lines is not the density's, so
        // two messages are that and the halved room apart: 6 where it was 8.
        XCTAssertEqual(tight.within * 2, MessageMeasures.leading + tight.underName)
        XCTAssertEqual(tight.within * 2, 6)
        XCTAssertEqual(roomy.within * 2, 8)
    }

    // MARK: Stored

    func testAPhoneThatHasNeverChosenFollowsTheSystemAndIsComfortable() {
        let preferences = Preferences(defaults: scratch(), environment: [:])
        XCTAssertEqual(preferences.appearance, .system)
        XCTAssertEqual(preferences.density, .comfortable)
        XCTAssertEqual(Appearance.standard, .system)
        XCTAssertEqual(Density.standard, .comfortable)
    }

    func testEachChoiceIsKeptForTheNextLaunch() {
        let defaults = scratch()
        let preferences = Preferences(defaults: defaults, environment: [:])
        preferences.appearance = .dark
        preferences.density = .compact
        XCTAssertEqual(defaults.string(forKey: Preferences.appearanceKey), "dark")
        XCTAssertEqual(defaults.string(forKey: Preferences.densityKey), "compact")
        // A second launch over the same store has them, before anything is asked.
        let again = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(again.appearance, .dark)
        XCTAssertEqual(again.density, .compact)
        // And back: System is stored as a choice, not as nothing.
        again.appearance = .system
        again.density = .comfortable
        XCTAssertEqual(defaults.string(forKey: Preferences.appearanceKey), "system")
        XCTAssertEqual(Preferences(defaults: defaults, environment: [:]).appearance, .system)
        XCTAssertEqual(Preferences(defaults: defaults, environment: [:]).density, .comfortable)
    }

    func testWhatIsStoredAndNotUnderstoodReadsAsTheDefault() {
        let defaults = scratch()
        defaults.set("sepia", forKey: Preferences.appearanceKey)
        defaults.set("Compact", forKey: Preferences.densityKey)
        let preferences = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(preferences.appearance, .system)
        XCTAssertEqual(preferences.density, .comfortable)
        // Not a string at all.
        defaults.set(2, forKey: Preferences.appearanceKey)
        defaults.set(["compact"], forKey: Preferences.densityKey)
        XCTAssertEqual(Preferences(defaults: defaults, environment: [:]).appearance, .system)
        XCTAssertEqual(Preferences(defaults: defaults, environment: [:]).density, .comfortable)
        XCTAssertEqual(Appearance(stored: nil), .system)
        XCTAssertEqual(Density(stored: ""), .comfortable)
    }

    func testAChoiceIsReadByWhatObservesIt() {
        let preferences = Preferences(defaults: scratch(), environment: [:])
        let changed = expectation(description: "the density's readers are told")
        withObservationTracking { _ = preferences.density } onChange: { changed.fulfill() }
        preferences.density = .compact
        wait(for: [changed], timeout: 1)
    }

    // The smoke runs' launches (LUMIA_DEV_APPEARANCE, LUMIA_DEV_DENSITY).
    func testALaunchCanNameTheChoicesWithoutStoringThem() {
        let defaults = scratch()
        defaults.set("light", forKey: Preferences.appearanceKey)
        let named = Preferences(defaults: defaults, environment: ["LUMIA_DEV_APPEARANCE": "dark", "LUMIA_DEV_DENSITY": "compact"])
        XCTAssertEqual(named.appearance, .dark)
        XCTAssertEqual(named.density, .compact)
        XCTAssertEqual(defaults.string(forKey: Preferences.appearanceKey), "light")
        XCTAssertNil(defaults.string(forKey: Preferences.densityKey))
        // The next launch without them reads what was there before.
        let next = Preferences(defaults: defaults, environment: [:])
        XCTAssertEqual(next.appearance, .light)
        XCTAssertEqual(next.density, .comfortable)
        // A word that is not a choice, or none, names nothing.
        let unnamed = Preferences(defaults: defaults, environment: ["LUMIA_DEV_APPEARANCE": "", "LUMIA_DEV_DENSITY": "tiny"])
        XCTAssertEqual(unnamed.appearance, .light)
        XCTAssertEqual(unnamed.density, .comfortable)
    }

    // The choices are this phone's and not the account's: ending the
    // session (signing out, or forgetting the instance) leaves them.
    func testSigningOutLeavesTheChoices() async {
        let (appearance, density) = (UserDefaults.standard.string(forKey: Preferences.appearanceKey), UserDefaults.standard.string(forKey: Preferences.densityKey))
        defer {
            UserDefaults.standard.set(appearance, forKey: Preferences.appearanceKey)
            UserDefaults.standard.set(density, forKey: Preferences.densityKey)
        }
        let preferences = Preferences(defaults: .standard, environment: [:])
        preferences.appearance = .dark
        preferences.density = .compact
        let store = AppStore()
        await store.signOut()
        await store.forgetInstance()
        XCTAssertEqual(store.route, .signIn)
        let after = Preferences(defaults: .standard, environment: [:])
        XCTAssertEqual(after.appearance, .dark)
        XCTAssertEqual(after.density, .compact)
    }
}
