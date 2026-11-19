// The things the simulator cannot do from the command line: answer the
// notification permission prompt, tap a notification, type into a
// composer, make a choice in Preferences, sign out from the account menu,
// look into Notification Center, put a link request on the clipboard,
// choose a picture in the system's photo picker, tap twice, pinch and
// drag in the picture viewer, and read a screen as
// assistive technology does. scripts/smoke-push.sh, scripts/smoke-media.sh,
// scripts/smoke-typing.sh, scripts/smoke-preferences.sh,
// scripts/smoke-signout.sh, scripts/smoke-server-settings.sh, scripts/smoke-rail.sh,
// scripts/smoke-drafts.sh and scripts/smoke-approve.sh run these tests with the app signed in
// (the LUMIA_DEV_* environment, passed through xcodebuild's TEST_RUNNER_
// prefix); scripts/smoke-viewer.sh runs its test with no account at all.
import UIKit
import XCTest

final class SmokeUITests: XCTestCase {
    /// Launches the app with the runner's `LUMIA_DEV_*` environment, `screen`
    /// in place of its `LUMIA_DEV_SCREEN` and `appearance` in place of its
    /// `LUMIA_DEV_APPEARANCE` when given.
    private func launch(screen: String? = nil, appearance: String? = nil) -> XCUIApplication {
        let app = XCUIApplication()
        for (key, value) in ProcessInfo.processInfo.environment where key.hasPrefix("LUMIA_DEV_") {
            app.launchEnvironment[key] = value
        }
        if let screen { app.launchEnvironment["LUMIA_DEV_SCREEN"] = screen }
        if let appearance { app.launchEnvironment["LUMIA_DEV_APPEARANCE"] = appearance }
        app.launch()
        return app
    }

    /// Preferences by hand (scripts/smoke-preferences.sh): the account menu
    /// on the rail's own avatar leads there, each choice is an element that
    /// says whether it is the one selected (Images, Embeds and Reactions
    /// switches), the preview is one picture, a choice takes effect as it is
    /// made and is there at the next launch, which draws a channel by it.
    /// A switch turned leaves no preset's card lit and says "Custom";
    /// Minimal chosen writes its values, lights its card and takes the
    /// Avatars row away; a setting changed after it leaves no preset
    /// selected and says "Custom". The smoke looks at the screen
    /// at each step announced under LUMIA_UITEST_STEPS: `menu`, `dark`,
    /// `kept` (a channel, dark, in one line, cozy and at 18 from what was
    /// stored alone) and `system`.
    func testPreferencesAreChosenAndKept() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        // A step is announced, then held for the smoke's screenshot of it.
        let reached = { (step: String) in
            try? step.write(toFile: "\(steps)/\(step)", atomically: true, encoding: .utf8)
            sleep(4)
        }
        var app = launch(screen: "home")
        /// Opens Preferences from the account menu, whose items are in the
        /// web's and the desktop shell's order from the top.
        func openPreferences() {
            let account = app.descendants(matching: .any).matching(NSPredicate(format: "label ENDSWITH %@", ", account")).firstMatch
            XCTAssertTrue(account.waitForExistence(timeout: 90), "no account menu on the rail")
            account.tap()
            // The system's menu is a collection of buttons (Home has a Devices of its own).
            let items = ["View profile", "Preferences", "Devices"].map { app.collectionViews.buttons[$0] }
            for item in items { XCTAssertTrue(item.waitForExistence(timeout: 10), "the account menu has no \(item)") }
            XCTAssertEqual(items.map(\.frame.minY), items.map(\.frame.minY).sorted(), "View profile, Preferences, Devices from the top")
        }
        /// One of a group's choices (a picture, a preset's card, a segment),
        /// inside the group that says what is being chosen.
        func choice(_ group: String, _ label: String) -> XCUIElement {
            app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", group)).buttons[label]
        }
        /// Which of a group's choices say they are selected; none of a group not drawn.
        func selected(_ group: String, of labels: [String]) -> [String] {
            labels.filter { let c = choice(group, $0); return c.exists && c.isSelected }
        }
        let groups: [(String, [String])] = [
            ("Appearance", ["Light", "Dark", "System"]),
            ("Preset", ["Comfortable", "Compact", "Minimal"]),
            ("Layout", ["Grouped", "One line"]),
            ("Spacing", ["Roomy", "Cozy", "Tight"]),
            ("Avatars", ["Large", "Small", "Hidden"]),
            ("Font size", ["13 points", "14 points", "15 points", "16 points", "18 points", "20 points"]),
        ]
        let switches = ["Images", "Embeds", "Reactions"]
        let labels = Dictionary(uniqueKeysWithValues: groups)
        /// A switch, by the name of its row.
        func toggle(_ name: String) -> XCUIElement { app.switches[name] }
        /// What each group has selected, from the top, a group not drawn having
        /// none, and each switch "on" or "off".
        func chosen() -> [String: [String]] {
            var all = Dictionary(uniqueKeysWithValues: groups.map { ($0.0, selected($0.0, of: $0.1)) })
            for name in switches { all[name] = [(toggle(name).value as? String) == "1" ? "on" : "off"] }
            return all
        }
        let custom = "Custom: these settings match no preset."
        // Images, Embeds and Reactions are on together, or off together, in every preset.
        let page = { (dark: String, preset: [String], layout: String, spacing: String, avatars: [String], size: String, on: Bool) -> [String: [String]] in
            let state = on ? "on" : "off"
            return ["Appearance": [dark], "Preset": preset, "Layout": [layout], "Spacing": [spacing], "Avatars": avatars, "Font size": ["\(size) points"], "Images": [state], "Embeds": [state], "Reactions": [state]]
        }
        let preview = app.descendants(matching: .any)["Preview of messages with these settings"]

        openPreferences()
        _ = reached("menu")
        app.collectionViews.buttons["Preferences"].tap()
        for words in ["Preferences", "How Lumia looks on this phone", "Appearance", "Light, dark, or whichever this phone is in.", "Messages", "Start from a preset, then change anything below.", "Kept on this phone only. Your other devices and your account are not changed."] {
            XCTAssertTrue(app.staticTexts[words].waitForExistence(timeout: 10), "the screen does not say \"\(words)\"")
        }
        for (group, all) in groups { for label in all { XCTAssertTrue(choice(group, label).exists, "no \(label) under \(group)") } }
        for name in switches { XCTAssertTrue(toggle(name).exists, "no \(name) switch") }
        XCTAssertFalse(app.staticTexts["Scroll bars"].exists, "a phone's scroll indicators are the system's")
        // The preview is one picture to assistive technology: its messages,
        // names, chips and card are found only inside it (a UI test still
        // lists what is under an element; VoiceOver stops at the element).
        XCTAssertTrue(preview.exists, "no preview of the messages")
        XCTAssertEqual(preview.elementType, .image)
        for inside in ["Priya Nair", "design-review", "reacted with", "What’s new in Lumia 0.13"] {
            let named = NSPredicate(format: "label CONTAINS %@", inside)
            XCTAssertTrue(preview.descendants(matching: .any).matching(named).firstMatch.exists, "the preview does not draw \"\(inside)\"")
            XCTAssertEqual(app.descendants(matching: .any).matching(named).count, preview.descendants(matching: .any).matching(named).count, "\"\(inside)\" is drawn outside the preview")
        }
        // As a new phone has them.
        XCTAssertEqual(chosen(), page("System", ["Comfortable"], "Grouped", "Roomy", ["Large"], "15", true))
        XCTAssertFalse(app.staticTexts[custom].exists)

        // A switch turned from a preset leaves no card lit and says Custom; turned back, Comfortable is lit again.
        /// Whether the preview draws something labelled with `words`.
        func previewed(_ words: String) -> Bool {
            preview.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", words)).firstMatch.exists
        }
        let card = "What’s new in Lumia 0.13"
        toggle("Embeds").tap()
        XCTAssertEqual((toggle("Embeds").value as? String), "0", "Embeds did not turn off")
        XCTAssertEqual(selected("Preset", of: labels["Preset"]!), [])
        XCTAssertTrue(app.staticTexts[custom].waitForExistence(timeout: 5), "the screen does not say \"\(custom)\"")
        XCTAssertFalse(previewed(card), "the preview draws the card with Embeds off")
        toggle("Embeds").tap()
        XCTAssertEqual(selected("Preset", of: labels["Preset"]!), ["Comfortable"])
        XCTAssertFalse(app.staticTexts[custom].exists)
        XCTAssertTrue(previewed(card), "the preview draws no card with Embeds on")

        choice("Appearance", "Dark").tap()
        // Minimal writes its values, its three switches off among them, and lights its card; one line has no avatars to choose.
        choice("Preset", "Minimal").tap()
        XCTAssertEqual(chosen(), page("Dark", ["Minimal"], "One line", "Tight", [], "15", false))
        XCTAssertFalse(choice("Avatars", "Large").exists, "one line draws no avatar")
        XCTAssertFalse(app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Avatars")).firstMatch.exists, "Avatars is a row in one line")
        // And the preview follows: the picture a file row, no reactions, no card.
        XCTAssertTrue(previewed("File, onboarding-first-screen.png"), "the preview draws no file row with Images as links")
        XCTAssertFalse(previewed("reacted with"), "the preview draws reactions in Minimal")
        XCTAssertFalse(previewed(card), "the preview draws the card in Minimal")
        // A setting changed after it is Custom, and the font size is no preset's.
        choice("Font size", "18 points").tap()
        XCTAssertEqual(selected("Preset", of: labels["Preset"]!), ["Minimal"])
        choice("Spacing", "Cozy").tap()
        XCTAssertEqual(chosen(), page("Dark", [], "One line", "Cozy", [], "18", false))
        XCTAssertTrue(app.staticTexts[custom].waitForExistence(timeout: 5), "the screen does not say \"\(custom)\"")
        _ = reached("dark")

        // The next launch names no choice: what it draws is what was kept.
        app.terminate()
        app = launch(screen: "channel")
        XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(format: "placeholderValue BEGINSWITH %@", "Message #")).firstMatch.waitForExistence(timeout: 90), "the channel did not open")
        // In one line each message is still one stop that says whose it is and when, then what.
        let row = app.descendants(matching: .any).matching(NSPredicate(format: "value == %@", "Two, a moment later.")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 30), "no row says the second message")
        XCTAssertTrue(row.label.contains(", "), "the row's label \"\(row.label)\" is not whose and when")
        _ = reached("kept")
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch.tap()
        openPreferences()
        app.collectionViews.buttons["Preferences"].tap()
        XCTAssertTrue(choice("Appearance", "Dark").waitForExistence(timeout: 10))
        XCTAssertEqual(chosen(), page("Dark", [], "One line", "Cozy", [], "18", false))
        XCTAssertTrue(app.staticTexts[custom].exists)

        // And back, as it is made: the phone's own appearance again, Comfortable at 15.
        choice("Appearance", "System").tap()
        choice("Preset", "Comfortable").tap()
        choice("Font size", "15 points").tap()
        XCTAssertEqual(chosen(), page("System", ["Comfortable"], "Grouped", "Roomy", ["Large"], "15", true))
        XCTAssertFalse(app.staticTexts[custom].exists)
        _ = reached("system")
    }

    /// Sign out by hand (scripts/smoke-signout.sh). The account menu ends
    /// with Sign out under its rule, a button by that name; tapping it
    /// shows the sign-in screen on the instance the phone knows, without
    /// the line for someone the instance sent back, and takes the account's
    /// notifications out of Notification Center. Typing another account's
    /// password there signs in as that account, in the same run of the app:
    /// its name on the rail, its server and channel, and nothing of the
    /// first's. That one signs out as the first did, the first signs in
    /// again by hand, and then the instance ends its session from outside:
    /// the sign-in screen with its line, and again no notification left.
    /// The launch after that names no account and opens on sign-in. The
    /// smoke looks at the screen and at the instance at each step announced
    /// under LUMIA_UITEST_STEPS, and the test goes on once the smoke says it
    /// has (`<step>.seen`): `in`, `menu-light`, `menu-dark`, `out`, `other`,
    /// `out-other`, `back`, `noticed`, `ended`, `next-launch`.
    func testSignOutFromTheAccountMenu() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let handle = try XCTUnwrap(env["LUMIA_DEV_HANDLE"])
        let password = try XCTUnwrap(env["LUMIA_DEV_PASSWORD"])
        let host = try XCTUnwrap(env["LUMIA_UITEST_HOST"])
        // The first account's server and channel, the second account and
        // its own, and what the two notifications the smoke delivers say.
        let server = try XCTUnwrap(env["LUMIA_UITEST_SERVER"])
        let channel = try XCTUnwrap(env["LUMIA_UITEST_CHANNEL"])
        let other = try XCTUnwrap(env["LUMIA_UITEST_OTHER"])
        let otherServer = try XCTUnwrap(env["LUMIA_UITEST_OTHER_SERVER"])
        let otherChannel = try XCTUnwrap(env["LUMIA_UITEST_OTHER_CHANNEL"])
        let notice = try XCTUnwrap(env["LUMIA_UITEST_NOTICE"])
        let lastNotice = try XCTUnwrap(env["LUMIA_UITEST_LAST_NOTICE"])
        let strayNotice = try XCTUnwrap(env["LUMIA_UITEST_STRAY_NOTICE"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        var app = XCUIApplication()
        let account = { app.descendants(matching: .any).matching(NSPredicate(format: "label ENDSWITH %@", ", account")).firstMatch }
        let signOut = { app.collectionViews.buttons["Sign out"] }
        let endedLine = { app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "You were signed out")).firstMatch }
        /// Opens the account menu: the web's and the desktop shell's items from the top, Sign out last.
        func openMenu() {
            XCTAssertTrue(account().waitForExistence(timeout: 90), "no account menu on the rail")
            account().tap()
            let items = ["View profile", "Preferences", "Devices", "Sign out"].map { app.collectionViews.buttons[$0] }
            for item in items { XCTAssertTrue(item.waitForExistence(timeout: 10), "the account menu has no \(item)") }
            XCTAssertEqual(items.map(\.frame.minY), items.map(\.frame.minY).sorted(), "View profile, Preferences, Devices, Sign out from the top")
            // As assistive technology has it: a button, by that name, that can be pressed.
            XCTAssertEqual(signOut().elementType, .button)
            XCTAssertEqual(signOut().label, "Sign out")
            XCTAssertTrue(signOut().isEnabled)
        }
        /// The sign-in screen on the instance the phone knows: with nothing
        /// said about why after a sign-out the person chose, and with its
        /// line when the instance ended the session.
        func expectSignIn(_ when: String, ended: Bool = false) {
            XCTAssertTrue(app.staticTexts["Welcome back"].waitForExistence(timeout: 60), "no sign-in screen \(when)")
            XCTAssertTrue(app.staticTexts[host].waitForExistence(timeout: 10), "the sign-in screen \(when) is not on \(host)")
            if ended {
                XCTAssertTrue(endedLine().waitForExistence(timeout: 10), "the sign-in screen \(when) does not say the instance ended the session")
            } else {
                XCTAssertFalse(endedLine().exists, "the sign-in screen \(when) says the instance ended the session")
            }
            XCTAssertFalse(account().exists, "the rail is still there \(when)")
        }
        /// Signs in by typing into the sign-in screen, and expects that account's name on the rail.
        func signIn(as who: String) {
            let name = app.textFields["you@example.com"]
            XCTAssertTrue(name.waitForExistence(timeout: 10), "no field for the handle")
            name.tap()
            name.typeText(who)
            let secret = app.secureTextFields["Your password"]
            secret.tap()
            secret.typeText(password + "\n")
            // The system may offer to keep the password; the rail is behind its sheet.
            let later = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Not Now"]
            for _ in 0..<60 where !account().exists {
                if later.exists { later.tap() }
                if app.buttons["Not Now"].exists { app.buttons["Not Now"].tap() }
                sleep(1)
            }
            XCTAssertTrue(account().exists, "typing \(who)'s password did not sign in")
            XCTAssertEqual(account().label, "\(who), account", "the rail is not \(who)'s")
        }
        /// The rail and the sidebar are one account's: its server and
        /// channel, and nothing that names the other's.
        func expectOnly(_ mine: (server: String, channel: String), not theirs: (server: String, channel: String), _ when: String) {
            XCTAssertTrue(app.buttons[mine.server].waitForExistence(timeout: 30), "no \(mine.server) on the rail \(when)")
            app.buttons[mine.server].tap()
            XCTAssertTrue(app.staticTexts[mine.channel].waitForExistence(timeout: 10), "no #\(mine.channel) in \(mine.server) \(when)")
            for words in [theirs.server, theirs.channel] {
                XCTAssertFalse(app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", words)).firstMatch.exists, "something says \(words) \(when)")
            }
        }

        // Signed in from the runner's environment, as the other smokes are,
        // in an appearance named for the launch: the menu in each.
        app = launch(screen: "home", appearance: "light")
        XCTAssertTrue(account().waitForExistence(timeout: 90), "the app did not sign in")
        XCTAssertEqual(account().label, "\(handle), account")
        // A new phone asks about notifications once; allowed, the system
        // gives the app a token where it can, and the phone is subscribed.
        let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow"]
        if allow.waitForExistence(timeout: 30) { allow.tap() }
        // The smoke delivers a notification for this account before it says it has looked.
        reached("in")
        XCTAssertTrue(listed(notice, over: app, within: 30), "the notification delivered while signed in is not in Notification Center")
        expectOnly((server, channel), not: (otherServer, otherChannel), "signed in as \(handle)")
        openMenu()
        reached("menu-light")
        app.terminate()
        app = launch(screen: "home", appearance: "dark")
        openMenu()
        reached("menu-dark")

        signOut().tap()
        expectSignIn("after signing out")
        XCTAssertFalse(listed(notice, over: app, within: 0), "signed out, the account's notification is still in Notification Center")
        reached("out")
        // The smoke has pushed once more, to the signed-out phone with the
        // app in the foreground: nothing is shown and nothing is listed.
        let stray = XCUIApplication(bundleIdentifier: "com.apple.springboard").descendants(matching: .any)
            .matching(NSPredicate(format: "label CONTAINS %@", strayNotice)).firstMatch
        XCTAssertFalse(stray.waitForExistence(timeout: 8), "a push to the signed-out phone was shown")
        XCTAssertFalse(listed(strayNotice, over: app, within: 0), "a push to the signed-out phone is in Notification Center")

        // Another account, by hand, in the same run of the app: what the
        // phone shows is that account's and nothing of the one before.
        signIn(as: other)
        expectOnly((otherServer, otherChannel), not: (server, channel), "signed in as \(other)")
        reached("other")

        openMenu()
        signOut().tap()
        expectSignIn("after the second account signed out")
        reached("out-other")

        // The first again, by hand: what the phone forgot of the account at
        // sign-out, it asks the instance again. The smoke delivers another
        // notification before it says it has looked.
        signIn(as: handle)
        expectOnly((server, channel), not: (otherServer, otherChannel), "signed in as \(handle) again")
        reached("back")
        XCTAssertTrue(listed(lastNotice, over: app, within: 30), "the second notification is not in Notification Center")

        // The smoke revokes the session over HTTP once it reads this: the
        // instance sends the phone back to sign-in, which says so, and the
        // notification goes as it does at a sign-out the person chose.
        reached("noticed")
        expectSignIn("after the instance ended the session", ended: true)
        XCTAssertFalse(listed(lastNotice, over: app, within: 0), "sent back to sign-in, the account's notification is still in Notification Center")
        reached("ended")

        // The next launch names no account: what it shows is what the phone kept.
        app.terminate()
        app = XCUIApplication()
        app.launch()
        expectSignIn("at the next launch")
        reached("next-launch")
    }

    /// Signing out of an instance that takes the connection and answers
    /// nothing (scripts/smoke-signout.sh, whose listener stands between the
    /// phone and the instance and goes quiet once this is signed in): the
    /// core gives each request a time limit, so the sign-in screen is there
    /// well within a minute, with nothing of the account on screen
    /// meanwhile. Steps: `quiet-in`, and `quiet-out`, which carries the
    /// seconds from the tap to the sign-in screen.
    func testSignOutWhenTheInstanceDoesNotAnswer() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        let app = launch(screen: "home")
        let account = app.descendants(matching: .any).matching(NSPredicate(format: "label ENDSWITH %@", ", account")).firstMatch
        XCTAssertTrue(account.waitForExistence(timeout: 90), "the app did not sign in through the listener")
        // A moment for what the shell asks as it appears to be answered.
        sleep(5)
        reached("quiet-in", under: steps)

        account.tap()
        let signOut = app.collectionViews.buttons["Sign out"]
        XCTAssertTrue(signOut.waitForExistence(timeout: 10), "no Sign out in the account menu")
        signOut.tap()
        let tapped = Date()
        // Reaching the instance fails as well, so the screen asks which
        // instance ("Welcome") where it would otherwise say "Welcome back".
        let signIn = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Welcome")).firstMatch
        var railGone: TimeInterval?
        while !signIn.exists, Date().timeIntervalSince(tapped) < 90 {
            if railGone == nil, !account.exists { railGone = Date().timeIntervalSince(tapped) }
            usleep(200_000)
        }
        let took = Date().timeIntervalSince(tapped)
        XCTAssertTrue(signIn.exists, "no sign-in screen \(Int(took)) s after Sign out")
        XCTAssertLessThan(took, 45, "signing out of an instance that does not answer took \(Int(took)) s")
        XCTAssertLessThan(try XCTUnwrap(railGone, "the rail was there until the sign-in screen"), 5, "the account stayed on screen while the sign-out waited")
        XCTAssertFalse(account.exists)
        reached("quiet-out", under: steps, saying: String(format: "%.0f", took))
    }

    /// Server settings by hand (scripts/smoke-server-settings.sh), signed in
    /// as the server's owner on the server, LUMIA_UITEST_SERVER: "Server
    /// settings" under its channels leads to the screen, whose words, field
    /// and choices are checked as assistive technology reads them; the name
    /// becomes LUMIA_UITEST_RENAME and 30 days is chosen, which says what it
    /// will delete before Save and nothing after. The smoke then gives the
    /// server a picture from elsewhere, which the screen and the rail draw.
    /// Show deleted messages, off, is turned on. Files says what they take,
    /// and keeping attachments 30 days and deleting the oldest when full
    /// each say what they will delete before Save; once saved, only what
    /// the chosen one does stays said under it.
    /// Steps (each waits for `<step>.seen`): `open`, `warning`, `saved`
    /// (the smoke reads the server from the instance and sets the picture),
    /// `shown` (the smoke reads `showDeleted` from the instance), `files`
    /// (which carries the usage line; the smoke reads the file settings and
    /// the storage from the instance), `picture`, and `rail`, which carries where the server's tile is, as
    /// fractions of the window: "x y width height".
    func testServerSettingsAreChangedHere() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let serverName = try XCTUnwrap(env["LUMIA_UITEST_SERVER"])
        let rename = try XCTUnwrap(env["LUMIA_UITEST_RENAME"])
        let app = launch(screen: "server")
        let anything = app.descendants(matching: .any)
        let settings = app.buttons["Server settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 90), "no Server settings for the server's owner")
        settings.tap()
        // Its sections are a screen of their own (architecture 7.5); Overview is the first.
        XCTAssertTrue(anything.matching(NSPredicate(format: "label == %@", "Server settings")).firstMatch.waitForExistence(timeout: 10), "the screen does not say \"Server settings\"")
        let overview = app.buttons["section-overview"]
        XCTAssertTrue(overview.waitForExistence(timeout: 10), "Server settings does not offer Overview")
        overview.tap()

        for words in [serverName, "Overview", "Choose a picture…", "Name", "Message history", "How long this server keeps its messages."] {
            XCTAssertTrue(anything.matching(NSPredicate(format: "label == %@", words)).firstMatch.waitForExistence(timeout: 10), "the screen does not say \"\(words)\"")
        }
        XCTAssertTrue(app.images["No server picture"].exists || anything.matching(NSPredicate(format: "label == %@", "No server picture")).firstMatch.exists, "the picture is not labelled")
        XCTAssertFalse(app.buttons["Remove"].exists, "Remove is offered for a server with no picture")
        let field = app.textFields["Name"]
        XCTAssertTrue(field.exists, "no field called Name")
        XCTAssertEqual(field.value as? String, serverName)
        let saveName = app.buttons["save-name"]
        let saveRetention = app.buttons["save-retention"]
        XCTAssertEqual(saveName.label, "Save")
        XCTAssertFalse(saveName.isEnabled, "Save is offered for the name unchanged")
        XCTAssertFalse(saveRetention.isEnabled, "Save is offered for the retention unchanged")
        let choices = ["Forever", "1 year", "90 days", "30 days"]
        let choice = { (label: String) in anything.matching(NSPredicate(format: "label == %@", "Message history")).buttons[label] }
        for label in choices { XCTAssertTrue(choice(label).exists, "no \(label) under Message history") }
        XCTAssertEqual(choices.filter { choice($0).isSelected }, ["Forever"], "a new server keeps its messages for ever")
        reached("open", under: steps)

        // The name, trimmed.
        // Past the end of the name puts the caret after it; then it is deleted.
        field.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: 0.5)).tap()
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: serverName.count + 2))
        XCTAssertFalse(saveName.isEnabled, "Save is offered for an empty name")
        field.typeText("  \(rename) ")
        XCTAssertTrue(saveName.isEnabled)
        saveName.tap()
        let renamed = anything.matching(NSPredicate(format: "label == %@", rename)).firstMatch
        XCTAssertTrue(renamed.waitForExistence(timeout: 20), "the header does not say the new name")
        XCTAssertFalse(saveName.isEnabled, "Save is still offered once the name is saved")
        XCTAssertEqual(field.value as? String, rename, "the field does not show the name trimmed")
        XCTAssertTrue(app.staticTexts["Saved."].waitForExistence(timeout: 5), "nothing says the name was saved")
        // Return puts the keyboard away, so the rest of the screen is in view.
        field.typeText("\n")

        // 30 days, shorter than for ever: Save says first what it will delete.
        let warning = anything.matching(NSPredicate(format: "label == %@", "Messages older than 30 days will be deleted for everyone. This cannot be undone.")).firstMatch
        XCTAssertFalse(warning.exists, "a warning before anything shorter is chosen")
        app.swipeUp()
        choice("30 days").tap()
        XCTAssertEqual(choices.filter { choice($0).isSelected }, ["30 days"])
        XCTAssertTrue(warning.waitForExistence(timeout: 5), "nothing says what 30 days will delete")
        XCTAssertTrue(saveRetention.isEnabled)
        reached("warning", under: steps)
        saveRetention.tap()
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: warning)
        XCTAssertEqual(XCTWaiter().wait(for: [gone], timeout: 20), .completed, "the warning is still there once 30 days is saved")
        XCTAssertFalse(saveRetention.isEnabled, "Save is still offered once the retention is saved")
        XCTAssertEqual(choices.filter { choice($0).isSelected }, ["30 days"])
        reached("saved", under: steps)

        // Show deleted messages, off for a new server, saved as it is turned.
        let caption = "Off, a deleted message disappears for everyone. On, it stays in its place as “(deleted)”."
        let shown = app.switches.matching(NSPredicate(format: "label BEGINSWITH %@", "Show deleted messages")).firstMatch
        if !shown.exists { app.swipeUp() }
        XCTAssertTrue(shown.waitForExistence(timeout: 10), "no switch for Show deleted messages")
        XCTAssertTrue(shown.label.contains(caption), "the switch does not say what it does: \(shown.label)")
        XCTAssertEqual(shown.value as? String, "0", "a new server shows its deleted messages")
        // The switch itself, at the row's end: its words do not turn it.
        shown.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap()
        let on = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "1"), object: shown)
        XCTAssertEqual(XCTWaiter().wait(for: [on], timeout: 10), .completed, "the switch did not turn on")
        let saved = app.staticTexts["show-deleted-saved"]
        XCTAssertTrue(saved.waitForExistence(timeout: 20), "nothing under the switch says it was saved")
        XCTAssertEqual(saved.label, "Saved.")
        XCTAssertEqual(shown.value as? String, "1", "the switch went back: the instance refused it")
        reached("shown", under: steps)

        // Files (specs/media.md 4.5): what they take, then 30 days and the
        // oldest let go when full, each saying first what it will delete.
        let usage = app.staticTexts["storage-usage"]
        for _ in 0..<3 where !(usage.exists && usage.isHittable) { app.swipeUp() }
        XCTAssertTrue(usage.waitForExistence(timeout: 20), "the Files card does not say what the server's files take")
        XCTAssertTrue(usage.label.hasSuffix(" used"), "the usage line says \"\(usage.label)\"")
        let kept = { (label: String) in anything.matching(NSPredicate(format: "label == %@", "Keep attachments for")).buttons[label] }
        for label in choices { XCTAssertTrue(kept(label).exists, "no \(label) under Keep attachments for") }
        XCTAssertEqual(choices.filter { kept($0).isSelected }, ["Forever"], "a new server keeps its attachments for ever")
        let full = { (label: String) in anything.matching(NSPredicate(format: "label == %@", "When the server is full")).buttons[label] }
        let fullChoices = ["Refuse new uploads", "Delete the oldest attachments"]
        for label in fullChoices { XCTAssertTrue(full(label).exists, "no \(label) under When the server is full") }
        XCTAssertEqual(fullChoices.filter { full($0).isSelected }, ["Refuse new uploads"], "a new server refuses uploads when full")
        let saveFiles = app.buttons["save-files"]
        XCTAssertFalse(saveFiles.isEnabled, "Save is offered for the files unchanged")
        let filesWarning = anything.matching(NSPredicate(format: "label == %@", "Attachments older than 30 days will be deleted for everyone. Their messages stay. This cannot be undone.")).firstMatch
        // Under the choice, what the chosen one does.
        let refuses = anything.matching(NSPredicate(format: "label == %@", "When an upload would not fit, it is refused until files are deleted.")).firstMatch
        let fullWarning = anything.matching(NSPredicate(format: "label == %@", "When an upload would not fit, the oldest messages’ attachments are deleted for everyone to make room.")).firstMatch
        XCTAssertTrue(refuses.exists, "nothing says what refusing new uploads does")
        XCTAssertFalse(filesWarning.exists || fullWarning.exists, "a warning before anything is chosen")
        kept("30 days").tap()
        XCTAssertTrue(filesWarning.waitForExistence(timeout: 5), "nothing says what 30 days will delete")
        if !full("Delete the oldest attachments").isHittable { app.swipeUp() }
        full("Delete the oldest attachments").tap()
        XCTAssertEqual(fullChoices.filter { full($0).isSelected }, ["Delete the oldest attachments"])
        XCTAssertTrue(fullWarning.waitForExistence(timeout: 5), "nothing says what deleting the oldest does")
        if !saveFiles.isHittable { app.swipeUp() }
        XCTAssertTrue(saveFiles.isEnabled)
        saveFiles.tap()
        let warningGone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: filesWarning)
        XCTAssertEqual(XCTWaiter().wait(for: [warningGone], timeout: 20), .completed, "the retention's warning is still there once the files are saved")
        // What deleting the oldest does is still said, now as a description.
        XCTAssertTrue(fullWarning.exists, "nothing says what the chosen Delete the oldest attachments does once saved")
        XCTAssertFalse(saveFiles.isEnabled, "Save is still offered once the files are saved")
        reached("files", under: steps, saying: usage.label)

        // The smoke has given the server a picture from elsewhere; the
        // account event brings it here.
        for _ in 0..<3 { app.swipeDown() }
        let remove = app.buttons["Remove"]
        XCTAssertTrue(remove.waitForExistence(timeout: 30), "the picture set elsewhere did not reach the screen")
        XCTAssertTrue(anything.matching(NSPredicate(format: "label == %@", "Server picture")).firstMatch.exists, "the picture is not labelled")
        reached("picture", under: steps)

        // Back over the sections to the sidebar.
        let backButton = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch
        while backButton.exists { backButton.tap(); sleep(1) }
        let tile = app.buttons[rename]
        XCTAssertTrue(tile.waitForExistence(timeout: 10), "no \(rename) on the rail")
        XCTAssertTrue(tile.isSelected)
        let window = app.windows.firstMatch.frame
        let f = tile.frame
        reached("rail", under: steps, saying: String(format: "%.4f %.4f %.4f %.4f", (f.minX - window.minX) / window.width, (f.minY - window.minY) / window.height, f.width / window.width, f.height / window.height))
    }

    /// A member whose roles do not let them change the server's settings is
    /// not offered them (scripts/smoke-server-settings.sh signs in as one).
    /// Step `member`.
    func testServerSettingsAreNotOfferedToAMember() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        let app = launch(screen: "server")
        XCTAssertTrue(app.buttons["Invite people"].waitForExistence(timeout: 90), "the server did not open")
        XCTAssertFalse(app.buttons["Server settings"].exists, "Server settings is offered to a member who may not change them")
        reached("member", under: steps)
    }

    /// Roles, titles and tags by hand (scripts/smoke-roles.sh, which seeds
    /// the boards' Acme: the phone's account owning it, five roles, six
    /// members with roles and titles, a guest, a morning in #general). Each
    /// step is announced and waits for `<step>.seen`, the smoke reading the
    /// instance at it: `sections` (Server settings' three, with what each
    /// holds), `made` (New role: Support, in sky, saved from the header),
    /// `asks` (Administrator turned on asks first), `edited` (renamed
    /// Helpers, Manage messages and Administrator on, "Saved."), `menu`
    /// (the role's More), `moved` (Move up, drawn at once), `given` (Add
    /// members: Priya), `removed` (the Remove beside Priya in Design's
    /// editor), `titled` (Leo's row in Members: a title),
    /// `tags` (#general, each author's tag beside their name; the smoke then
    /// takes Admin from Maya from elsewhere), `live` (her tag is Design with
    /// the conversation still on screen), `card` (Devon's profile card: his
    /// title and his roles, each changed from its Edit), `own` (this
    /// account's own card: "You" and "Owner" both, and a title with a hidden
    /// character refused in the editor), `titles` (Overview's "Tag beside
    /// names": Titles, and #general tags by title), `kicked` (Sam, from his
    /// row, nothing asked), `deleting` (the role's sheet, saying who holds
    /// it) and `deleted`.
    func testRolesTitlesAndTagsByHand() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        let app = launch(screen: "server")
        let anything = app.descendants(matching: .any)
        func labelled(_ words: String) -> XCUIElement { anything.matching(NSPredicate(format: "label == %@", words)).firstMatch }
        func starting(_ words: String) -> XCUIElement { anything.matching(NSPredicate(format: "label BEGINSWITH %@", words)).firstMatch }
        let backButton = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch
        func back() { backButton.tap(); sleep(1) }
        func shown(_ element: XCUIElement, _ why: String, within seconds: TimeInterval = 15) { XCTAssertTrue(element.waitForExistence(timeout: seconds), why) }
        func gone(_ element: XCUIElement, _ why: String, within seconds: TimeInterval = 15) {
            XCTAssertEqual(XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: element)], timeout: seconds), .completed, why)
        }
        /// Brings something further down the screen into reach.
        func reach(_ element: XCUIElement) { for _ in 0..<6 where !(element.exists && element.isHittable) { app.swipeUp(velocity: .slow) } }
        /// Types over what a field holds.
        func retype(_ field: XCUIElement, _ words: String) {
            reach(field)
            let held = (field.value as? String) ?? ""
            field.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: 0.5)).tap()
            field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: held.count + 2) + words)
        }

        // Server settings: its sections, each with what it holds.
        let settings = app.buttons["Server settings"]
        shown(settings, "no Server settings for the server's owner", within: 90)
        settings.tap()
        let roles = app.buttons["section-roles"], members = app.buttons["section-members"]
        shown(app.buttons["section-overview"], "Server settings does not offer Overview")
        shown(roles, "Server settings does not offer Roles")
        XCTAssertEqual(roles.label, "Roles")
        XCTAssertEqual(roles.value as? String, "5 roles")
        XCTAssertEqual(members.label, "Members")
        // Counted once the member list has been read.
        XCTAssertEqual(XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", "7"), object: members)], timeout: 20), .completed, "Members does not say how many: \(String(describing: members.value))")
        reached("sections", under: steps)

        // Roles, in order, each saying how many hold it; Everyone last, with no More.
        roles.tap()
        shown(labelled("In order, highest first. A role is moved from its menu. A new role starts last, just above Everyone."), "Roles does not say how its order works")
        let order = ["Admin", "Moderator", "Design", "Engineering", "Can invite", "Everyone"]
        for name in order { shown(app.buttons["role-\(name)"], "no row for \(name)") }
        XCTAssertEqual(order.sorted { app.buttons["role-\($0)"].frame.minY < app.buttons["role-\($1)"].frame.minY }, order, "the roles are not listed highest first")
        XCTAssertEqual(app.buttons["role-Moderator"].value as? String, "1 member")
        XCTAssertEqual(app.buttons["role-Can invite"].value as? String, "2 members, not shown as tag")
        XCTAssertEqual(app.buttons["role-Everyone"].value as? String, "7 members")
        XCTAssertTrue(app.buttons["more-Design"].exists, "Design has no More")
        XCTAssertFalse(app.buttons["more-Everyone"].exists, "Everyone has a More")

        // New role: nothing to save until it has a name; then it is the role this editor is of.
        app.buttons["New role"].tap()
        let save = app.buttons["Save"]
        shown(save, "a new role's editor has no Save")
        XCTAssertFalse(save.isEnabled, "Save is offered for a role with no name")
        XCTAssertFalse(app.buttons["delete-role"].exists, "a new role offers Delete role")
        let name = app.textFields["Name"]
        name.tap()
        // A name another role has, whatever its capitals: refused, and said under the header.
        name.typeText("admin")
        save.tap()
        shown(app.staticTexts["role-error"], "a name another role has was not refused")
        XCTAssertEqual(app.staticTexts["role-error"].label, "Another role in Acme has that name.")
        XCTAssertFalse(app.buttons["delete-role"].exists, "a role refused is taken as made")
        retype(name, "Support")
        let colours = anything.matching(NSPredicate(format: "label == %@", "Colour"))
        XCTAssertEqual(["Peach", "Blue", "Purple", "Mint", "Amber", "Sky", "Stone", "Green", "Indigo", "None"].filter { !colours.buttons[$0].exists }, [], "a colour is missing")
        XCTAssertTrue(colours.buttons["None"].isSelected, "a new role has a colour")
        colours.buttons["Sky"].tap()
        XCTAssertTrue(colours.buttons["Sky"].isSelected)
        let preview = labelled("Preview")
        XCTAssertTrue((preview.value as? String)?.hasSuffix(", Support") ?? false, "the preview does not carry the tag: \(String(describing: preview.value))")
        XCTAssertTrue(save.isEnabled)
        save.tap()
        shown(app.buttons["delete-role"], "the role made is not the one the editor is of")
        shown(labelled("Nobody holds this role yet."), "a new role does not say nobody holds it")
        reached("made", under: steps)

        // Edit it: its name, a permission, and Administrator, which asks first.
        name.typeText("\n")
        retype(name, "Helpers\n")
        let manageMessages = app.switches["permission-MANAGE_MESSAGES"], administrator = app.switches["permission-ADMINISTRATOR"]
        reach(administrator)
        XCTAssertEqual(administrator.value as? String, "0")
        administrator.tap()
        shown(labelled("Turn on Administrator?"), "turning Administrator on asked nothing")
        shown(starting("Administrator allows everything in Acme, in every channel, whatever a channel’s own permissions say."), "the question does not say what Administrator allows")
        reached("asks", under: steps)
        app.buttons["Cancel"].tap()
        gone(labelled("Turn on Administrator?"), "the question did not go")
        XCTAssertEqual(administrator.value as? String, "0", "Administrator went on though the question was cancelled")
        administrator.tap()
        shown(app.buttons["ask-confirm"], "turning Administrator on asked nothing the second time")
        XCTAssertEqual(app.buttons["ask-confirm"].label, "Turn on")
        app.buttons["ask-confirm"].tap()
        gone(labelled("Turn on Administrator?"), "the question did not go")
        XCTAssertEqual(administrator.value as? String, "1", "Administrator is not on after Turn on")
        reach(manageMessages)
        manageMessages.tap()
        XCTAssertEqual(manageMessages.value as? String, "1")
        XCTAssertTrue(save.isEnabled)
        save.tap()
        shown(app.staticTexts["role-saved"], "nothing says the role was saved")
        XCTAssertFalse(save.isEnabled, "Save is still offered once the role is saved")
        shown(app.staticTexts["Helpers"], "the header does not say the role's new name")
        reached("edited", under: steps)

        // Its More: Move up puts it above Can invite, drawn at once.
        back()
        shown(app.buttons["role-Helpers"], "Roles does not list Helpers")
        app.buttons["more-Helpers"].tap()
        for item in ["Edit role", "Move up", "Move down", "Delete role"] { shown(app.buttons[item], "the role's menu has no \(item)", within: 5) }
        XCTAssertFalse(app.buttons["Move down"].isEnabled, "Move down is offered to the last role above Everyone")
        reached("menu", under: steps)
        app.buttons["Move up"].tap()
        let above = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in app.buttons["role-Helpers"].frame.minY < app.buttons["role-Can invite"].frame.minY }, object: nil)
        XCTAssertEqual(XCTWaiter().wait(for: [above], timeout: 10), .completed, "Helpers did not move above Can invite")
        XCTAssertFalse(app.staticTexts["roles-error"].exists, "the move was refused")
        reached("moved", under: steps)

        // Its editor's Add members: Priya, who then holds it.
        app.buttons["role-Helpers"].tap()
        let add = app.buttons["Add members"]
        reach(add)
        add.tap()
        let priya = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Priya Natarajan")).firstMatch
        shown(priya, "Add members does not list Priya")
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Jordan Lee")).firstMatch.exists, "a guest is offered a role")
        priya.tap()
        app.buttons["Add"].tap()
        shown(app.buttons["Remove Priya Natarajan from Helpers"], "the editor does not list Priya among those who hold the role")
        reached("given", under: steps)

        // Remove, beside one who holds a role: Priya loses Design at once.
        back()
        app.buttons["role-Design"].tap()
        let removePriya = app.buttons["Remove Priya Natarajan from Design"]
        reach(removePriya)
        XCTAssertTrue(app.buttons["Remove Maya Chen from Design"].exists, "Design's editor does not list Maya among those who hold it")
        removePriya.tap()
        gone(removePriya, "Priya is still listed among those who hold Design")
        reached("removed", under: steps)

        // Members: Leo's row opens to his roles, his title and Kick.
        back(); back()
        members.tap()
        let leo = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "member-leo")).firstMatch
        shown(leo, "Members does not list Leo")
        XCTAssertTrue((leo.value as? String)?.hasSuffix("Mobile Lead, Can invite, collapsed") ?? false, "Leo's row says \(String(describing: leo.value))")
        let owner = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "member-ryan")).firstMatch
        XCTAssertTrue((owner.value as? String)?.hasPrefix("Owner, ") ?? false, "the owner's row does not say so: \(String(describing: owner.value))")
        let guest = anything.matching(NSPredicate(format: "identifier BEGINSWITH %@", "member-jordan")).firstMatch
        XCTAssertTrue((guest.value as? String)?.hasPrefix("Guest, ") ?? false, "the guest's row does not say so: \(String(describing: guest.value))")
        // A search over the list held: by title.
        let search = app.textFields["Search members"]
        search.tap()
        search.typeText("mobile")
        gone(owner, "a search for a title still lists everyone")
        XCTAssertTrue(leo.exists, "a search for Leo's title does not find him")
        retype(search, "\n")
        shown(owner, "the list did not come back once the search was emptied")
        reach(leo)
        leo.tap()
        let title = app.textFields["Title"]
        shown(title, "Leo's row did not open to his title")
        XCTAssertEqual(title.value as? String, "Mobile Lead")
        for role in ["Admin", "Moderator", "Design", "Engineering", "Helpers", "Can invite"] { XCTAssertTrue(app.buttons["tick-\(role)"].exists, "no \(role) to tick") }
        XCTAssertEqual(app.buttons["tick-Can invite"].value as? String, "ticked")
        XCTAssertEqual(app.buttons["tick-Design"].value as? String, "not ticked")
        XCTAssertTrue(app.buttons["member-kick"].exists, "the owner is not offered Kick for Leo")
        shown(labelled("Up to 32 characters. A title grants nothing."), "nothing says what a title is")
        retype(title, "Release Captain\n")
        reach(app.buttons["member-save"])
        app.buttons["member-save"].tap()
        gone(title, "the row did not close once saved")
        let titled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value CONTAINS %@", "Release Captain"), object: leo)
        XCTAssertEqual(XCTWaiter().wait(for: [titled], timeout: 15), .completed, "Leo's row does not say his new title: \(String(describing: leo.value))")
        reached("titled", under: steps)

        // #general: each author's tag beside their name, and it follows a change made elsewhere.
        back(); back()
        app.buttons["general"].tap()
        let maya = starting("Maya Chen, ")
        shown(maya, "#general has no message of Maya's")
        XCTAssertTrue(maya.label.hasPrefix("Maya Chen, Admin, "), "Maya's message says whose it is as \"\(maya.label)\"")
        XCTAssertTrue(starting("Devon Park, Moderator, ").exists, "Devon's message carries no Moderator tag")
        XCTAssertTrue(starting("Priya Natarajan, Helpers, ").exists, "Priya's tag is not the role made and given to her, her highest shown as a tag")
        XCTAssertTrue(starting("Leo Martins, ").exists && !starting("Leo Martins, Can invite").exists, "a role not shown as a tag is Leo's tag")
        XCTAssertTrue(starting("Ryan Winchester, ").exists && !starting("Ryan Winchester, Founder").exists, "the owner carries a title as a tag where the server shows roles")
        reached("tags", under: steps)
        shown(starting("Maya Chen, Design, "), "Maya's tag did not follow her roles changed from elsewhere while the conversation was open", within: 30)
        reached("live", under: steps)

        // Devon's card, from the channel's members: his title and his roles, each changed from its Edit.
        app.buttons["Members"].tap()
        let devon = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Devon Park")).firstMatch
        shown(devon, "the channel's members do not list Devon")
        XCTAssertTrue(devon.label.contains("Moderator"), "Devon's row in the channel's members carries no tag: \(devon.label)")
        devon.tap()
        let editTitle = app.buttons["Edit title"], editRoles = app.buttons["Edit roles"]
        shown(editTitle, "the card does not offer the owner Devon's title")
        XCTAssertTrue(app.staticTexts["Staff Engineer"].exists, "the card does not show Devon's title")
        XCTAssertTrue(labelled("Moderator, Engineering").exists, "the card does not list Devon's roles")
        editTitle.tap()
        shown(labelled("Devon Park in Acme"), "the title's editor does not say whose it is")
        retype(app.textFields["Title"], "Principal Engineer\n")
        app.buttons["member-save"].tap()
        shown(app.staticTexts["Principal Engineer"], "the card does not show Devon's new title")
        editRoles.tap()
        shown(app.buttons["tick-Design"], "the card's Edit did not open Devon's roles")
        XCTAssertFalse(app.buttons["member-kick"].exists, "a card's editor offers Kick")
        app.buttons["tick-Design"].tap()
        XCTAssertEqual(app.buttons["tick-Design"].value as? String, "ticked")
        reach(app.buttons["member-save"])
        app.buttons["member-save"].tap()
        shown(labelled("Moderator, Design, Engineering"), "the card does not list the role Devon was given")
        reached("card", under: steps)
        putAwaySheet(app, showing: editRoles)

        // This account's own card: "You" and who it is to the server, both
        // said; and a title with a character in it that nobody can see is
        // refused in the editor, in the sheet's words, and is not its title.
        let mine = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Ryan Winchester")).firstMatch
        shown(mine, "the channel's members do not list this account")
        mine.tap()
        shown(editTitle, "this account's own card does not offer its title")
        let marks = app.staticTexts.matching(identifier: "card-mark")
        XCTAssertEqual((0..<marks.count).map { marks.element(boundBy: $0).label }, ["You", "Owner"], "the owner's own card does not say both You and Owner")
        XCTAssertTrue(app.staticTexts["Founder"].exists, "the card does not show this account's title")
        editTitle.tap()
        shown(labelled("Ryan Winchester in Acme"), "the title's editor does not say whose it is")
        retype(app.textFields["Title"], "Foun\u{200B}der in chief\n")
        app.buttons["member-save"].tap()
        shown(app.staticTexts["member-error"], "a title with a hidden character in it was not refused")
        XCTAssertEqual(app.staticTexts["member-error"].label, "A title is up to 32 characters on one line, with no hidden characters.")
        reached("own", under: steps)
        app.buttons["Cancel"].tap()
        shown(app.staticTexts["Founder"], "the card does not show the title as it was")
        putAwaySheet(app, showing: editTitle)
        back()

        // Overview: the tag beside names is the server's to choose; Titles.
        back()
        settings.tap()
        app.buttons["section-overview"].tap()
        let tagChoices = anything.matching(NSPredicate(format: "label == %@", "Tag beside names"))
        let byRoles = tagChoices.buttons["Roles"], byTitles = tagChoices.buttons["Titles"]
        shown(tagChoices.firstMatch, "Overview has no Tag beside names")
        reach(byTitles)
        XCTAssertTrue(byRoles.isSelected, "a server does not show roles until told otherwise")
        byTitles.tap()
        let chosen = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isSelected == true"), object: byTitles)
        XCTAssertEqual(XCTWaiter().wait(for: [chosen], timeout: 15), .completed, "Titles was not chosen")
        XCTAssertFalse(byRoles.isSelected)
        shown(labelled("A profile shows a member’s title and roles either way."), "nothing says what a profile shows")
        back(); back()
        app.buttons["general"].tap()
        shown(starting("Maya Chen, Head of Design, "), "Maya's tag is not her title once the server shows titles")
        XCTAssertTrue(starting("Ryan Winchester, Founder, ").exists, "the owner's title is not their tag")
        XCTAssertTrue(starting("Priya Natarajan, ").exists && !starting("Priya Natarajan, Design").exists, "a member with no title falls back to a role")
        reached("titles", under: steps)

        // Kick, from a member's row: nothing is asked first.
        back()
        settings.tap()
        members.tap()
        let sam = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "member-sam")).firstMatch
        shown(sam, "Members does not list Sam")
        reach(sam)
        sam.tap()
        let kick = app.buttons["member-kick"]
        reach(kick)
        XCTAssertEqual(kick.label, "Kick Sam Okafor")
        kick.tap()
        gone(sam, "Sam is still listed after Kick")
        reached("kicked", under: steps)

        // Delete role, from its More: the sheet says who holds it.
        back()
        roles.tap()
        shown(app.buttons["more-Helpers"], "Roles does not list Helpers")
        app.buttons["more-Helpers"].tap()
        app.buttons["Delete role"].tap()
        shown(labelled("Delete Helpers?"), "deleting a role asked nothing")
        shown(labelled("1 member holds this role. They lose what it allowed. This can’t be undone."), "the sheet does not say who holds the role")
        XCTAssertEqual(app.buttons["ask-cancel"].label, "Keep it")
        reached("deleting", under: steps)
        app.buttons["ask-cancel"].tap()
        gone(labelled("Delete Helpers?"), "the sheet did not go")
        XCTAssertTrue(app.buttons["role-Helpers"].exists, "the role went though it was kept")
        app.buttons["more-Helpers"].tap()
        app.buttons["Delete role"].tap()
        shown(app.buttons["ask-confirm"], "deleting a role asked nothing the second time")
        XCTAssertEqual(app.buttons["ask-confirm"].label, "Delete role")
        app.buttons["ask-confirm"].tap()
        gone(app.buttons["role-Helpers"], "Helpers is still listed after Delete role")
        reached("deleted", under: steps)
    }

    /// Roles as someone who manages only what is below them
    /// (scripts/smoke-roles.sh signs in as Devon, whose highest role is
    /// Moderator, with Manage roles and Manage members): Server settings
    /// has Roles and Members and no Overview; Admin and Moderator are
    /// listed and not opened or moved; in a role's editor Administrator is
    /// not Devon's to turn and says why; and of Maya, who is above, only
    /// the roles below Devon's own can be given and taken. Step `below`.
    func testRolesAsOneWhoManagesWhatIsBelow() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        let app = launch(screen: "server")
        let anything = app.descendants(matching: .any)
        let backButton = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch
        let settings = app.buttons["Server settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 90), "no Server settings for one who manages roles and members")
        settings.tap()
        XCTAssertTrue(app.buttons["section-roles"].waitForExistence(timeout: 10), "no Roles for one who manages roles")
        XCTAssertTrue(app.buttons["section-members"].exists, "no Members for one who manages members")
        XCTAssertFalse(app.buttons["section-overview"].exists, "Overview is offered to one who may not manage the server")
        app.buttons["section-roles"].tap()

        // At or above Devon's own highest: listed, and neither opened nor moved.
        XCTAssertTrue(app.buttons["role-Design"].waitForExistence(timeout: 10), "Roles does not list Design")
        for name in ["Admin", "Moderator"] {
            XCTAssertFalse(app.buttons["role-\(name)"].exists, "\(name) opens for one at or below it")
            XCTAssertFalse(app.buttons["more-\(name)"].exists, "\(name) has a More for one at or below it")
            let row = anything.matching(NSPredicate(format: "label == %@ AND value CONTAINS %@", name, "not yours to change")).firstMatch
            XCTAssertTrue(row.exists, "\(name)'s row does not say it is not Devon's to change")
        }
        XCTAssertTrue(app.buttons["more-Design"].exists && app.buttons["more-Engineering"].exists, "the roles below have no More")
        // Design is the highest Devon manages: it goes no higher.
        app.buttons["more-Design"].tap()
        XCTAssertTrue(app.buttons["Move down"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Move up"].isEnabled, "Design may be moved above Moderator")
        XCTAssertTrue(app.buttons["Move down"].isEnabled)
        app.buttons["Edit role"].tap()

        // Administrator is not Devon's to give: shown, not turned, and saying why.
        let administrator = app.switches["permission-ADMINISTRATOR"]
        for _ in 0..<4 where !(administrator.exists && administrator.isHittable) { app.swipeUp(velocity: .slow) }
        XCTAssertTrue(administrator.exists, "the editor does not show Administrator")
        XCTAssertFalse(administrator.isEnabled, "Administrator may be turned by one who does not hold it")
        // Why is the switch's hint, which a UI test cannot read; the smoke's picture `devon-editor` shows it.
        let mention = app.switches["permission-MENTION_EVERYONE"]
        for _ in 0..<4 where !(mention.exists && mention.isHittable) { app.swipeUp(velocity: .slow) }
        XCTAssertTrue(mention.isEnabled, "a permission Devon holds is not Devon's to give")

        // Maya is above Devon: her roles below Devon's own can be given and taken, and nothing else.
        backButton.tap(); sleep(1); backButton.tap(); sleep(1)
        app.buttons["section-members"].tap()
        let maya = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "member-maya")).firstMatch
        XCTAssertTrue(maya.waitForExistence(timeout: 10), "Members does not list Maya")
        maya.tap()
        XCTAssertTrue(app.buttons["tick-Design"].waitForExistence(timeout: 10), "Maya's row did not open to her roles")
        XCTAssertFalse(app.buttons["tick-Admin"].isEnabled, "Admin may be given by one below it")
        XCTAssertFalse(app.buttons["tick-Moderator"].isEnabled, "Moderator may be given by one who holds it as their highest")
        XCTAssertTrue(app.buttons["tick-Design"].isEnabled && app.buttons["tick-Engineering"].isEnabled)
        XCTAssertFalse(app.textFields["Title"].exists, "the title of someone above is offered")
        XCTAssertFalse(app.buttons["member-kick"].exists, "Kick is offered for someone above")
        reached("below", under: steps)
    }

    /// Channel membership by hand (scripts/smoke-membership.sh, which seeds
    /// the server: an invitation into #design from Ada, #random to browse,
    /// Ada taking adds as invitations, Grace in #general). Each step is
    /// announced and waits for `<step>.seen`, the smoke reading the instance
    /// at it: `invitation` (joined #design from the sidebar's row),
    /// `browsed` (#random read from Browse, then joined from its bar),
    /// `added` (Grace added to #random and Ada invited, from Add people),
    /// `removed` (Grace removed by the Remove at her row's end), `dm` (a server DM with
    /// Ada from her profile card, and a message sent there), `switch` (Let
    /// members of this server message me, off), `defaults` (#design made a
    /// default channel) and `left` (#random left from its members).
    func testChannelMembershipByHand() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        let app = launch(screen: "server")
        let anything = app.descendants(matching: .any)
        func labelled(_ words: String) -> XCUIElement { anything.matching(NSPredicate(format: "label == %@", words)).firstMatch }
        func button(containing words: String) -> XCUIElement { app.buttons.matching(NSPredicate(format: "label CONTAINS %@", words)).firstMatch }
        let backButton = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch
        func back() { backButton.tap() }
        /// Back to the server's sidebar, whatever is pushed over it.
        func sidebar() {
            while backButton.exists { backButton.tap(); sleep(1) }
            XCTAssertTrue(labelled("Direct messages").waitForExistence(timeout: 10), "not back at the server's sidebar")
        }

        // The invitation, above the channels: Join opens #design with its composer.
        XCTAssertTrue(labelled("Direct messages").waitForExistence(timeout: 90), "the server's sidebar has no Direct messages")
        let invitation = anything.matching(NSPredicate(format: "label ENDSWITH %@", "invited you")).firstMatch
        XCTAssertTrue(invitation.waitForExistence(timeout: 30), "no invitation in the sidebar")
        XCTAssertTrue(invitation.label.hasPrefix("#design · "), "the invitation reads \"\(invitation.label)\"")
        app.buttons["Join"].firstMatch.tap()
        XCTAssertTrue(composer(in: app, "Message #design").waitForExistence(timeout: 20), "joining from the invitation did not open #design with its composer")
        reached("invitation", under: steps)
        back()

        // Browse: #random read before joining, the bar in place of the composer.
        app.buttons["Add to Channels"].tap()
        app.buttons["Browse channels"].firstMatch.tap()
        let random = button(containing: "#random")
        XCTAssertTrue(random.waitForExistence(timeout: 20), "Browse does not list #random")
        XCTAssertTrue(random.label.contains("member"), "Browse does not say how many are in #random: \(random.label)")
        random.tap()
        XCTAssertTrue(labelled("You’re not in #random").waitForExistence(timeout: 20), "no bar saying the account is not in #random")
        XCTAssertFalse(composer(in: app, "Message #random").exists, "a composer in a channel the account is not in")
        app.buttons["Join"].firstMatch.tap()
        XCTAssertTrue(composer(in: app, "Message #random").waitForExistence(timeout: 20), "joining from the bar did not bring the composer back")
        reached("browsed", under: steps)

        // Add people: Grace added, Ada invited.
        app.buttons["Members"].tap()
        app.buttons["Add people"].firstMatch.tap()
        let grace = button(containing: "Grace Hopper")
        let ada = button(containing: "Ada Lovelace")
        XCTAssertTrue(grace.waitForExistence(timeout: 20), "Add people does not offer Grace")
        XCTAssertTrue(ada.label.contains("Will be invited"), "Ada, who takes adds as invitations, is not marked: \(ada.label)")
        XCTAssertFalse(app.buttons["Add"].isEnabled, "Add is offered with nobody chosen")
        grace.tap()
        ada.tap()
        app.buttons["Add"].tap()
        let graceRow = button(containing: "Grace Hopper")
        XCTAssertTrue(graceRow.waitForExistence(timeout: 20), "Grace is not in #random's members after the add")
        XCTAssertTrue(labelled("Members").exists, "Add did not go back to the members")
        reached("added", under: steps)

        // Removed by the Remove at the row's end, as the owner may (a swipe offers the same).
        let remove = app.buttons["Remove Grace Hopper from #random"]
        XCTAssertTrue(remove.exists, "Grace's row has no Remove at its end")
        remove.tap()
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: graceRow)
        XCTAssertEqual(XCTWaiter().wait(for: [gone], timeout: 20), .completed, "Grace is still listed once removed")
        reached("removed", under: steps)

        // Ada's profile card from #general's members: Message opens the server DM.
        sidebar()
        XCTAssertTrue(app.buttons["Ada Lovelace"].firstMatch.exists, "no server DM with Ada in the sidebar")
        app.buttons["general"].tap()
        app.buttons["Members"].tap()
        button(containing: "Ada Lovelace").tap()
        XCTAssertTrue(app.buttons["Message privately"].waitForExistence(timeout: 10), "the profile card offers no Message privately")
        app.buttons["Message"].tap()
        let composer = composer(in: app, "Message Ada Lovelace")
        XCTAssertTrue(composer.waitForExistence(timeout: 20), "Message did not open the server DM with Ada")
        XCTAssertTrue(anything.matching(NSPredicate(format: "label BEGINSWITH %@", "Messages here are kept by ")).firstMatch.exists, "the server DM does not say whose it is")
        composer.tap()
        composer.typeText("Sent from the phone's server DM.")
        app.buttons["Send"].tap()
        XCTAssertTrue(anything.matching(NSPredicate(format: "value == %@", "Sent from the phone's server DM.")).firstMatch.waitForExistence(timeout: 20), "the message sent is not in the server DM")
        reached("dm", under: steps)

        // Your settings: server DMs off, saved as it is turned.
        sidebar()
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Your settings in ")).firstMatch.tap()
        let dms = app.switches.matching(NSPredicate(format: "label BEGINSWITH %@", "Let members of this server message me")).firstMatch
        XCTAssertTrue(dms.waitForExistence(timeout: 10), "no switch for server DMs")
        XCTAssertEqual(dms.value as? String, "1", "server DMs are not on to begin with")
        // The switch itself, at the row's end: its words do not turn it.
        dms.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap()
        XCTAssertTrue(app.staticTexts["Saved."].waitForExistence(timeout: 10), "nothing says the switch was saved")
        XCTAssertEqual(dms.value as? String, "0")
        reached("switch", under: steps)
        back()

        // Default channels: #design too.
        app.buttons["Server settings"].tap()
        // Its sections are a screen of their own; Overview holds the card.
        XCTAssertTrue(app.buttons["section-overview"].waitForExistence(timeout: 10), "Server settings does not offer Overview")
        app.buttons["section-overview"].tap()
        XCTAssertTrue(labelled("Default channels").waitForExistence(timeout: 20))
        app.swipeUp()
        app.swipeUp()
        app.swipeUp()
        let design = button(containing: "#design")
        XCTAssertTrue(design.waitForExistence(timeout: 10), "Default channels does not list #design")
        XCTAssertFalse(design.isSelected)
        design.tap()
        app.buttons["save-defaults"].tap()
        XCTAssertTrue(app.staticTexts["Saved."].waitForExistence(timeout: 10), "nothing says the default channels were saved")
        reached("defaults", under: steps)
        back()
        back()

        // Leave #random from its members: back at the server, it is gone from the sidebar.
        app.buttons["random"].tap()
        app.buttons["Members"].tap()
        app.buttons["Leave channel"].tap()
        XCTAssertTrue(labelled("Direct messages").waitForExistence(timeout: 20), "leaving did not go back to the server")
        let left = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: app.buttons["random"])
        XCTAssertEqual(XCTWaiter().wait(for: [left], timeout: 20), .completed, "#random is still in the sidebar once left")
        reached("left", under: steps)
    }

    /// The rail's order and folders by hand (scripts/smoke-rail.sh, which
    /// seeds Acme, the folder Work of Northwind Studio, Open Source Guild,
    /// Design Systems Club and Lumia Beta, and Trail Runners, with three
    /// mentions in Work's servers and Northwind unread). Each step is
    /// announced and waits for `<step>.seen`, the smoke reading the rail from
    /// the instance at it: `closed` (Work one tile, its count three), `open`
    /// (tapped open, its servers in it), `menu` (Trail Runners' menu open),
    /// `moved` (Trail Runners moved up by it, over Work), `made` (Acme in a
    /// folder of its own by New folder, whose settings open and are
    /// cancelled), `settings` (Folder settings… from that folder's menu,
    /// Weekend typed and Sky chosen), `named` (saved) and
    /// `dragged` (Lumia Beta dragged out of Work to just over it).
    func testRailOrderAndFolders() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        let app = launch(screen: "server")
        let anything = app.descendants(matching: .any)
        let work = app.buttons["Folder, Work"]
        XCTAssertTrue(work.waitForExistence(timeout: 90), "no folder called Work on the rail")
        XCTAssertEqual(work.value as? String, "collapsed, 3 mentions")
        XCTAssertFalse(app.buttons["Northwind Studio"].exists, "a closed folder's servers are on the rail")
        XCTAssertTrue(app.buttons["Acme"].isSelected, "the server on screen is not Acme")
        reached("closed", under: steps)

        work.tap()
        let northwind = app.buttons["Northwind Studio"]
        XCTAssertTrue(northwind.waitForExistence(timeout: 5), "Work opened shows none of its servers")
        XCTAssertEqual(northwind.value as? String, "unread")
        XCTAssertEqual(app.buttons["Open Source Guild"].value as? String, "2 mentions")
        XCTAssertEqual(work.value as? String, "expanded, 3 mentions")
        reached("open", under: steps)

        // A long press let go opens the tile's menu, the system's sheet.
        let trail = app.buttons["Trail Runners"]
        trail.press(forDuration: 1.0)
        for item in ["Move up", "New folder", "Move to folder…"] {
            XCTAssertTrue(app.buttons[item].waitForExistence(timeout: 5), "Trail Runners' menu has no \(item)")
        }
        XCTAssertFalse(app.buttons["Move down"].exists, "the last tile can move down")
        XCTAssertFalse(app.buttons["Remove from folder"].exists, "a server in no folder can be removed from one")
        reached("menu", under: steps)
        app.buttons["Move up"].tap()
        XCTAssertTrue(above(trail, work), "Trail Runners did not move over Work")
        reached("moved", under: steps)

        // New folder: Acme in one of its own, whose settings follow.
        app.buttons["Acme"].press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["New folder"].waitForExistence(timeout: 5))
        app.buttons["New folder"].tap()
        let name = app.textFields["Name"]
        XCTAssertTrue(name.waitForExistence(timeout: 10), "Folder settings did not open for the new folder")
        XCTAssertEqual(name.placeholderValue, "Acme", "the name's placeholder is not its servers")
        XCTAssertTrue(anything.matching(NSPredicate(format: "label == %@", "Without a name it reads as its servers.")).firstMatch.exists)
        for tone in ["Peach", "Blue", "Purple", "Mint", "Amber", "Sky", "Stone", "Green", "Indigo"] {
            XCTAssertTrue(app.buttons[tone].exists, "no \(tone) among the colours")
        }
        XCTAssertEqual(["Peach", "Blue", "Purple", "Mint", "Amber", "Sky", "Stone", "Green", "Indigo"].filter { app.buttons[$0].isSelected }.count, 1, "not one colour chosen")
        app.buttons["Cancel"].tap()
        let made = app.buttons["Folder, Acme"]
        XCTAssertTrue(made.waitForExistence(timeout: 10), "no folder of Acme on the rail")
        reached("made", under: steps)

        // Folder settings… from the folder's menu: a name and a colour.
        made.press(forDuration: 1.0)
        for item in ["Move down", "Folder settings…", "Delete folder"] {
            XCTAssertTrue(app.buttons[item].waitForExistence(timeout: 5), "the folder's menu has no \(item)")
        }
        app.buttons["Folder settings…"].tap()
        XCTAssertTrue(name.waitForExistence(timeout: 10))
        name.tap()
        name.typeText("Weekend")
        app.buttons["Sky"].tap()
        XCTAssertTrue(app.buttons["Sky"].isSelected)
        reached("settings", under: steps)
        app.buttons["Save"].tap()
        XCTAssertTrue(app.buttons["Folder, Weekend"].waitForExistence(timeout: 10), "the folder is not called Weekend")
        reached("named", under: steps)

        // A drag: Lumia Beta out of Work, dropped just over Work's glyph, which is between Trail Runners and Work.
        let beta = app.buttons["Lumia Beta"]
        XCTAssertTrue(beta.exists)
        beta.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).press(forDuration: 1.0, thenDragTo: work.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: -0.25)))
        XCTAssertTrue(above(beta, work), "Lumia Beta was not dragged out over Work")
        reached("dragged", under: steps)
    }

    /// Whether `a` comes to stand over `b`, within ten seconds.
    private func above(_ a: XCUIElement, _ b: XCUIElement) -> Bool {
        for _ in 0..<50 {
            if a.exists, b.exists, a.frame.minY < b.frame.minY { return true }
            usleep(200_000)
        }
        return false
    }

    /// Announces a step to the smoke and waits until it says it has looked.
    /// The composer, by its identifier and, when given, its placeholder. Its field grows with what
    /// is typed (`axis: .vertical`), which iOS 17 shows assistive technology as a text view and
    /// iOS 18 and later as a text field, so it is looked for as either.
    private func composer(in app: XCUIApplication, _ placeholder: String? = nil) -> XCUIElement {
        let fields = app.descendants(matching: .any).matching(identifier: "composer")
        guard let placeholder else { return fields.firstMatch }
        return fields.matching(NSPredicate(format: "placeholderValue == %@ OR label == %@", placeholder, placeholder)).firstMatch
    }

    /// The composer for a test that reads it again and again: looked for
    /// among the text fields and the text views alone, waiting up to
    /// `seconds` for it. `composer(in:)` asks every element on screen for
    /// its identifiers, each of the keyboard's keys among them, and XCTest
    /// logs every answer from inside the app; a test that asks that often
    /// has the app's log quarantined for its volume on iOS 17.4, where
    /// XCTest's own handler for that then crashes the app.
    private func composerField(in app: XCUIApplication, within seconds: TimeInterval = 0) -> XCUIElement {
        let (field, view) = (app.textFields["composer"], app.textViews["composer"])
        let until = Date().addingTimeInterval(seconds)
        while true {
            if field.exists { return field }
            if view.exists || Date() >= until { return view }
            usleep(300_000)
        }
    }

    private func reached(_ step: String, under steps: String, saying words: String? = nil) {
        try? (words ?? step).write(toFile: "\(steps)/\(step)", atomically: true, encoding: .utf8)
        for _ in 0..<1200 where !FileManager.default.fileExists(atPath: "\(steps)/\(step).seen") { usleep(100_000) }
        XCTAssertTrue(FileManager.default.fileExists(atPath: "\(steps)/\(step).seen"), "the smoke never looked at \"\(step)\"")
    }

    /// Puts a sheet at its medium height away as a person does, by a tap
    /// over the conversation above it, and waits for `showing` to go.
    private func putAwaySheet(_ app: XCUIApplication, showing: XCUIElement) {
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.12)).tap()
        let gone = { XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: showing)], timeout: 5) }
        if gone() != .completed {
            app.swipeDown(velocity: .fast)
            XCTAssertEqual(gone(), .completed, "the sheet did not go")
        }
    }

    /// Whether Notification Center lists a notification that says `words`,
    /// looked at as a person does: pulled down from the top edge over the
    /// app, read, and pushed back up. `within` is how long one that is on
    /// its way is waited for.
    private func listed(_ words: String, over app: XCUIApplication, within seconds: TimeInterval) -> Bool {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let said = springboard.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", words)).firstMatch
        // What the pulled-down sheet shows over its list, there whether or
        // not anything is listed: without it nothing was looked at.
        let sheet = springboard.descendants(matching: .any)["lockscreen-date-view"]
        let until = Date().addingTimeInterval(seconds)
        // A banner over the app says the same words and is not the list: it
        // is gone in a few seconds.
        for _ in 0..<15 where said.exists { sleep(1) }
        var found = false
        repeat {
            springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.0))
                .press(forDuration: 0.1, thenDragTo: springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.85)))
            XCTAssertTrue(sheet.waitForExistence(timeout: 10), "Notification Center did not open")
            found = said.waitForExistence(timeout: 3)
            if ProcessInfo.processInfo.environment["LUMIA_UITEST_DUMP"] == "1" { print("notification center, looking for \(words):\n" + springboard.debugDescription) }
            springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.99))
                .press(forDuration: 0.1, thenDragTo: springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)))
            app.activate()
            _ = app.wait(for: .runningForeground, timeout: 10)
        } while !found && Date() < until
        return found
    }

    /// Accepts the prompt and keeps the app running while the link
    /// completes and the gateway publishes key packages. The app is on
    /// screen from the password's sign-in on, its gateway loop running, so
    /// the link's sign-in replaces a session in use: afterwards the phone is
    /// still in the app, not sent back to sign-in, and the sidebar's line
    /// says the gateway is connected, which it says of the new session's
    /// loop only, the old one having been told not to come back.
    func testAllowNotificationsAndHold() {
        let app = launch()
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let allow = springboard.buttons["Allow"]
        if allow.waitForExistence(timeout: 90) {
            allow.tap()
        }
        let hold = UInt32(ProcessInfo.processInfo.environment["LUMIA_UITEST_HOLD"] ?? "") ?? 20
        sleep(hold)
        XCTAssertEqual(app.state, .runningForeground)
        XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(format: "label ENDSWITH %@", ", account")).firstMatch.exists, "the rail is gone after linking")
        XCTAssertFalse(app.staticTexts["Welcome back"].exists, "linking sent the phone back to sign-in")
        let connected = app.staticTexts["connected"]
        if !connected.waitForExistence(timeout: 30) { print("the app without a connected gateway:\n" + app.debugDescription) }
        XCTAssertTrue(connected.exists, "the gateway is not connected after linking")
    }

    /// With the app in the foreground, the smoke delivers a push naming a
    /// conversation; the banner is tapped and the conversation's panel,
    /// titled LUMIA_UITEST_EXPECT, is what the app shows.
    func testOpenNotification() {
        let app = launch()
        let expected = ProcessInfo.processInfo.environment["LUMIA_UITEST_EXPECT"] ?? ""
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        // The smoke must not push before the app is up: a banner that
        // arrives on the home screen is gone by the time this looks. The
        // app in the foreground is announced through LUMIA_UITEST_READY.
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 90), "the app did not come to the foreground")
        if let ready = ProcessInfo.processInfo.environment["LUMIA_UITEST_READY"] {
            try? "ready".write(toFile: ready, atomically: true, encoding: .utf8)
        }
        // The banner is found by what it says (its identifier differs between iOS versions).
        let banner = springboard.descendants(matching: .any)
            .matching(NSPredicate(format: "label CONTAINS[c] %@", expected)).firstMatch
        // The smoke pushes a few seconds after the ready file; a dump while
        // the banner should be on screen says what is there instead.
        if !banner.waitForExistence(timeout: 10) {
            print("springboard 10 s after ready:\n" + springboard.debugDescription)
            print("the app 10 s after ready:\n" + app.debugDescription)
        }
        if !banner.waitForExistence(timeout: 80) {
            // What SpringBoard shows once the wait has run out, after the
            // notification has long been created: the banner under another
            // label, or nothing at all.
            print("springboard when the wait ran out:\n" + springboard.debugDescription)
            XCTFail("no banner arrived")
        }
        banner.tap()
        XCTAssertTrue(app.staticTexts[expected].waitForExistence(timeout: 30), "the conversation \(expected) did not open")
        // A moment for the smoke's screenshot of the opened panel.
        sleep(10)
    }

    /// Quote replies, reactions, mentions, read state, editing and deleting
    /// by hand (scripts/smoke-replies.sh), signed in as the owner of a server
    /// where Ada has written: #design unread with a mention, which opens at
    /// its "New messages" line and is read once its newest message is on
    /// screen; a long press's actions on Ada's message (Add reaction, Reply,
    /// Copy text, and Delete message for the owner, who may; no Edit
    /// message); a reply to it from there, with the reply bar's "@ On"
    /// turned off, drawn under its reference line; a reaction chip tapped on
    /// and off; one added through Add reaction's picker; the reply edited in
    /// place and then deleted after the confirmation, which takes its row
    /// away with no "(deleted)" left, the server not showing deleted
    /// messages; Bo picked from the @
    /// list, marked as not in #general, and added from the bar after the
    /// send; and the server's Mentions tile, whose page's item opens its
    /// channel. Each step waits for the smoke to look (`<step>.seen`):
    /// `sidebar`, `newline`, `read`, `actions`, `replybar`, `replied`, `reacted`,
    /// `unreacted`, `editing`, `edited`, `confirm`, `deleted`, `mentioned`, `added`, `mentionspage`.
    func testRepliesReactionsMentionsAndReads() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let bo = try XCTUnwrap(env["LUMIA_UITEST_OTHER"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        let app = launch(screen: "server")
        let anything = app.descendants(matching: .any)
        func labelled(_ words: String) -> XCUIElement { anything.matching(NSPredicate(format: "label == %@", words)).firstMatch }
        func starting(_ words: String) -> XCUIElement { anything.matching(NSPredicate(format: "label BEGINSWITH %@", words)).firstMatch }
        let backButton = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch
        let mentionsRow = app.buttons["mentions-row"]
        // The composer, by its identifier: with text in it, it reports no placeholder.
        let field = composer(in: app)
        func sidebar() {
            while backButton.exists { backButton.tap(); sleep(1) }
            XCTAssertTrue(mentionsRow.waitForExistence(timeout: 10), "not back at the server's sidebar")
        }
        func waitFor(_ element: XCUIElement, value: String, _ why: String) {
            let done = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@", value), object: element)
            XCTAssertEqual(XCTWaiter().wait(for: [done], timeout: 20), .completed, "\(why): \(element.value as? String ?? "nil")")
        }

        // The sidebar: the Mentions tile with the server's count, #design unread with its mention.
        XCTAssertTrue(mentionsRow.waitForExistence(timeout: 90), "no Mentions tile at the top of the server's sidebar")
        XCTAssertEqual(mentionsRow.label, "Mentions")
        XCTAssertTrue((mentionsRow.value as? String ?? "").contains("mention"), "the Mentions row has no count: \(mentionsRow.value ?? "nil")")
        let design = app.buttons["design"]
        XCTAssertTrue(design.waitForExistence(timeout: 20), "no #design in the sidebar")
        waitFor(design, value: "1 mention", "#design is not unread with one mention")
        reached("sidebar")

        // #design opens at its "New messages" line, and is read with its newest message on screen.
        design.tap()
        XCTAssertTrue(labelled("New messages").waitForExistence(timeout: 20), "no New messages line in #design")
        reached("newline")
        sleep(3)
        backButton.tap()
        waitFor(design, value: "", "#design is still unread after it was read")
        reached("read")

        // A long press on Ada's message: its actions, no quick reactions, and
        // no Edit message on someone else's; Reply puts the reply bar in the composer.
        app.buttons["general"].tap()
        let ada = anything.matching(NSPredicate(format: "label BEGINSWITH %@ AND value == %@", "Ada Lovelace, ", "Morning all, the review is at ten.")).firstMatch
        XCTAssertTrue(ada.waitForExistence(timeout: 30), "no message of Ada's in #general")
        ada.press(forDuration: 1.0)
        for words in ["Add reaction", "Reply", "Copy text", "Delete message"] {
            XCTAssertTrue(app.buttons[words].waitForExistence(timeout: 10), "the message's actions have no \(words)")
        }
        XCTAssertFalse(app.buttons["Edit message"].exists, "Edit message is offered on someone else's message")
        XCTAssertFalse(app.buttons["React with 👍"].exists, "the quick reactions are still in the sheet")
        reached("actions")
        app.buttons["Reply"].tap()
        XCTAssertTrue(labelled("Replying to Ada Lovelace").waitForExistence(timeout: 10), "no reply bar in the composer")
        XCTAssertTrue(app.buttons["Cancel reply"].exists)
        // "@ On", turned off: the reply notifies nobody.
        let mention = app.buttons["Mention Ada Lovelace"]
        XCTAssertTrue(mention.exists, "the reply bar has no @ On")
        XCTAssertEqual(mention.value as? String, "On", "@ On is not on for a new reply")
        mention.tap()
        waitFor(mention, value: "Off", "@ On did not turn off")
        reached("replybar")
        if app.buttons["Continue"].exists { app.buttons["Continue"].tap() }
        field.typeText("On it, thanks.")
        app.buttons["Send"].tap()
        let reply = anything.matching(NSPredicate(format: "label BEGINSWITH %@ AND value == %@", "Robin Owner, ", "On it, thanks.")).firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: 20), "the reply is not drawn")
        XCTAssertTrue(starting("Replying to Ada Lovelace: Morning all, the review is at ten. Jump to message").exists, "the reply has no reference line")
        XCTAssertFalse(app.buttons["Cancel reply"].exists, "the reply bar is still in the composer after the send")
        reached("replied")

        // A reaction chip: this account's added by a tap, and taken away by another.
        let thumbs = labelled("Ada Lovelace reacted with 👍, 1")
        XCTAssertTrue(thumbs.waitForExistence(timeout: 10), "no 👍 chip of Ada's")
        thumbs.tap()
        let mine = labelled("You and Ada Lovelace reacted with 👍, 2")
        XCTAssertTrue(mine.waitForExistence(timeout: 20), "the chip did not add this account's 👍")
        XCTAssertTrue(mine.isSelected, "the chip with this account's reaction does not say so")
        reached("reacted")
        mine.tap()
        XCTAssertTrue(labelled("Ada Lovelace reacted with 👍, 1").waitForExistence(timeout: 20), "the chip did not take this account's 👍 away")
        reached("unreacted")

        // Quote reply: a word selected in the actions' copy of the message, quoted alone.
        ada.press(forDuration: 1.0)
        let words = app.textViews.firstMatch
        XCTAssertTrue(words.waitForExistence(timeout: 10), "the message's actions do not show its words")
        // The first word, double-tapped once the sheet has settled: the
        // view's middle can fall between two words, which selects none.
        sleep(1)
        words.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.5)).doubleTap()
        let quoteReply = anything.matching(NSPredicate(format: "label == %@", "Quote reply")).firstMatch
        if !quoteReply.waitForExistence(timeout: 10) { print("the app with words selected:\n" + app.debugDescription) }
        XCTAssertTrue(quoteReply.exists, "the selection's menu has no Quote reply")
        quoteReply.tap()
        XCTAssertTrue(labelled("Replying to Ada Lovelace").waitForExistence(timeout: 10), "Quote reply put no reply bar in the composer")
        // The reply bar shows no quoted text: what was quoted is the reference line's once sent.
        if app.buttons["Continue"].exists { app.buttons["Continue"].tap() }
        field.typeText("Which part?")
        app.buttons["Send"].tap()
        // The word double-tapped, "Morning", and not the whole message, which the reply before quotes.
        XCTAssertTrue(labelled("Replying to Ada Lovelace: Morning. Jump to message").waitForExistence(timeout: 20), "the quote reply has no reference line quoting the word chosen alone")

        // The picker, from a long press's Add reaction: found by name.
        ada.press(forDuration: 1.0)
        // The sheet's, by its identifier: the chips under messages have an Add reaction of their own.
        let addReaction = app.buttons["action-addReaction"]
        XCTAssertTrue(addReaction.waitForExistence(timeout: 10), "the message's actions have no Add reaction")
        addReaction.tap()
        let search = app.textFields["Search emoji"]
        XCTAssertTrue(search.waitForExistence(timeout: 10), "no emoji picker")
        XCTAssertTrue(labelled("Frequently used").exists, "the picker has no Frequently used")
        search.tap()
        search.typeText("rocket")
        app.buttons["rocket"].firstMatch.tap()
        XCTAssertTrue(labelled("You reacted with 🚀, 1").waitForExistence(timeout: 20), "the picker's 🚀 is not on the message")

        // Editing one's own reply in place: the field holds its words with the
        // caret at their end, Return saves, and the message says "(edited)".
        reply.press(forDuration: 1.0)
        for words in ["Add reaction", "Reply", "Edit message", "Copy text", "Delete message"] {
            XCTAssertTrue(app.buttons[words].waitForExistence(timeout: 10), "one's own message's actions have no \(words)")
        }
        app.buttons["Edit message"].tap()
        // By its identifier and of any type: a field that grows is a text view on iOS 17 and a text field later.
        let editor = anything.matching(identifier: "message-editor").firstMatch
        XCTAssertTrue(editor.waitForExistence(timeout: 10), "Edit message put no field where the words were")
        XCTAssertEqual(editor.value as? String, "On it, thanks.")
        reached("editing")
        editor.typeText(" Done.\n")
        let edited = anything.matching(NSPredicate(format: "label BEGINSWITH %@ AND value == %@", "Robin Owner, ", "On it, thanks. Done., edited")).firstMatch
        XCTAssertTrue(edited.waitForExistence(timeout: 20), "the edit did not come back as the message's words, edited")
        XCTAssertFalse(editor.exists, "the field stayed after Return")
        reached("edited")

        // Deleting it, after the confirmation.
        edited.press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["Delete message"].waitForExistence(timeout: 10), "one's own message's actions have no Delete message")
        app.buttons["Delete message"].tap()
        let confirm = app.alerts["Delete this message?"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 10), "no confirmation before deleting")
        XCTAssertTrue(confirm.staticTexts["On it, thanks. Done."].exists, "the confirmation does not quote the message's first line")
        XCTAssertTrue(confirm.buttons["Cancel"].exists)
        reached("confirm")
        confirm.buttons["Delete"].tap()
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: edited)
        XCTAssertEqual(XCTWaiter().wait(for: [gone], timeout: 20), .completed, "the deleted message is still drawn with its words")
        // The server does not show deleted messages (off by default,
        // architecture 6.7): the row is gone, with no "(deleted)" in its
        // place, and so is the draft the owner deleted before; Ada's reply
        // to that draft still says what became of it.
        let tombstone = anything.matching(NSPredicate(format: "label BEGINSWITH %@ AND value == %@", "Robin Owner, ", "(deleted)")).firstMatch
        XCTAssertFalse(tombstone.exists, "the deleted reply is drawn as (deleted) in a server that does not show deleted messages")
        XCTAssertFalse(anything.matching(NSPredicate(format: "label == %@ OR value == %@", "(deleted)", "(deleted)")).firstMatch.exists, "something is drawn as (deleted) in #general")
        XCTAssertTrue(labelled("Original message was deleted").exists, "Ada's reply to the deleted draft does not say it was deleted")
        reached("deleted")

        // The @ list: Bo, who is not in #general, picked; the bar after the send adds him.
        // (A new simulator's keyboard first explains sliding to type.)
        if app.buttons["Continue"].exists { app.buttons["Continue"].tap() }
        if !field.waitForExistence(timeout: 10) { print("the app without its composer:\n" + app.debugDescription) }
        field.tap()
        field.typeText("@bo")
        let pick = starting("Bo Diddley @\(bo)")
        XCTAssertTrue(pick.waitForExistence(timeout: 10), "the @ list does not offer Bo")
        XCTAssertTrue(pick.label.hasSuffix("Not in #general"), "Bo is not marked as outside #general: \(pick.label)")
        pick.tap()
        XCTAssertEqual(field.value as? String, "@\(bo) ")
        field.typeText("please look")
        app.buttons["Send"].tap()
        let bar = labelled("Bo Diddley isn’t in #general, so they won’t be notified.")
        XCTAssertTrue(bar.waitForExistence(timeout: 20), "no bar saying Bo was not notified")
        reached("mentioned")
        app.buttons["Add to #general"].tap()
        let barGone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: bar)
        XCTAssertEqual(XCTWaiter().wait(for: [barGone], timeout: 20), .completed, "the bar stayed after Bo was added")
        reached("added")

        // The server's Mentions tile: Ada's messages for this account, each opening its channel.
        sidebar()
        mentionsRow.tap()
        let item = anything.matching(NSPredicate(format: "label BEGINSWITH %@", "Ada Lovelace in #general")).firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 20), "the Mentions page lists nothing of Ada's in #general")
        reached("mentionspage")
        item.tap()
        XCTAssertTrue(field.waitForExistence(timeout: 20), "the mention did not open #general")
    }

    /// The read wake (scripts/smoke-replies.sh, architecture 15.5): with
    /// notifications allowed and the app in the background, the smoke
    /// delivers three, one of #design (LUMIA_UITEST_READ), one of #general
    /// (LUMIA_UITEST_UNREAD) and the generic notice, and reads #design from
    /// elsewhere; then the read push arrives, here through the debug
    /// build's `lumia://dev/read-push?badge=N` (LUMIA_UITEST_WAKE), since
    /// `simctl push` refuses a silent push. Notification Center lists the
    /// notification of #general and no longer the one of #design. Steps:
    /// `wake-ready`, `woken`.
    func testTheReadWakeTakesAwayWhatIsRead() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let read = try XCTUnwrap(env["LUMIA_UITEST_READ"])
        let unread = try XCTUnwrap(env["LUMIA_UITEST_UNREAD"])
        let wake = try XCTUnwrap(env["LUMIA_UITEST_WAKE"].flatMap(URL.init(string:)))
        let app = launch(screen: "server")
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let allow = springboard.buttons["Allow"]
        if allow.waitForExistence(timeout: 60) { allow.tap() }
        XCTAssertTrue(app.buttons["mentions-row"].waitForExistence(timeout: 60), "the app did not open on the server")
        sleep(3)
        // Not running, as an app the system has suspended or ended is when a wake matters.
        app.terminate()
        reached("wake-ready", under: steps)
        app.open(wake)
        let open = springboard.buttons["Open"]
        if open.waitForExistence(timeout: 5) { open.tap() }
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 20))
        sleep(5)
        XCTAssertTrue(listed(unread, over: app, within: 10), "the notification of #general, still unread, is gone")
        XCTAssertFalse(listed(read, over: app, within: 0), "the notification of #design, read elsewhere, is still listed")
        reached("woken", under: steps)
    }

    /// A server's channel with someone else in it (scripts/smoke-typing.sh,
    /// which launches the app on the channel through LUMIA_DEV_SCREEN). The
    /// other person, LUMIA_UITEST_OTHER, has sent LUMIA_UITEST_ROWS messages
    /// in a row, the last of them LUMIA_UITEST_LAST: one block, whose rows
    /// under the first show no name and still say whose they are. Then the
    /// smoke has them type, and the line under the composer says so; then
    /// this types, which the smoke sees from the other side; then nobody
    /// does, and the line empties by itself. Each step is announced through
    /// a file under LUMIA_UITEST_STEPS.
    func testBlocksAndTypingInAChannel() throws {
        let env = ProcessInfo.processInfo.environment
        let other = try XCTUnwrap(env["LUMIA_UITEST_OTHER"])
        let rows = try XCTUnwrap(env["LUMIA_UITEST_ROWS"].flatMap(Int.init))
        let last = try XCTUnwrap(env["LUMIA_UITEST_LAST"])
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let reached = { (step: String) in try? step.write(toFile: "\(steps)/\(step)", atomically: true, encoding: .utf8) }
        let app = launch()
        let anything = app.descendants(matching: .any)

        // Every message of theirs is one element that begins with their name
        // and a time, whether or not its row draws them.
        let theirs = anything.matching(NSPredicate(format: "label BEGINSWITH %@", "\(other), "))
        let lastRow = theirs.matching(NSPredicate(format: "value == %@", last)).firstMatch
        if !lastRow.waitForExistence(timeout: 90) { print("the app without the block's last row:\n" + app.debugDescription) }
        XCTAssertTrue(lastRow.exists, "the block's last message does not say whose it is")
        XCTAssertEqual(theirs.count, rows, "one element for each message of \(other)")
        _ = reached("rows")

        // The smoke has the other person type from here on.
        let typing = anything.matching(NSPredicate(format: "label == %@", "\(other) is typing…")).firstMatch
        if !typing.waitForExistence(timeout: 60) { print("the app with nobody said to be typing:\n" + app.debugDescription) }
        XCTAssertTrue(typing.exists, "nothing says \(other) is typing")
        // A moment for the smoke's screenshot of the line.
        _ = reached("shown")
        sleep(3)

        // This person types: every edit says so to the core, which spaces
        // what goes out; the smoke watches the channel as the other person.
        let field = anything.matching(NSPredicate(format: "placeholderValue BEGINSWITH %@", "Message #")).firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10), "no composer")
        field.tap()
        field.typeText("Typed on the phone")
        _ = reached("typed")

        // The smoke stops typing once it reads that. Nothing arrives to say
        // so: the line empties when the last word of it runs out, about
        // eight seconds later.
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: typing)
        XCTAssertEqual(XCTWaiter().wait(for: [gone], timeout: 40), .completed, "the line still says \(other) is typing")
        _ = reached("quiet")
        // A moment for the smoke's screenshot of the composer with nothing under it.
        sleep(3)
    }

    /// What is typed and not sent is kept, by conversation (issue #23;
    /// scripts/smoke-drafts.sh, which seeds a server with #general, where
    /// LUMIA_UITEST_OTHER has asked something and this account,
    /// LUMIA_UITEST_ME, has a message of its own, and a server DM between
    /// the two). A reply is begun in #general, its "@ On" turned off, and
    /// words typed and not sent; the server DM's field is empty and takes
    /// words of its own; back in #general the words and the reply bar are
    /// as they were left. A message edited in place there is not the
    /// draft. More is typed and left to rest, and the smoke kills the app
    /// where it stands (`typed`), with no leaving: launched again, both
    /// conversations hold their words. Sent, #general's field is empty and
    /// stays empty through leaving, returning and another launch; the DM's
    /// field emptied by hand keeps nothing either. Each step is announced
    /// through a file under LUMIA_UITEST_STEPS and waits for the smoke.
    func testADraftIsKeptThroughASwitchAQuitAndASend() throws {
        let env = ProcessInfo.processInfo.environment
        let other = try XCTUnwrap(env["LUMIA_UITEST_OTHER"])
        let me = try XCTUnwrap(env["LUMIA_UITEST_ME"])
        let asked = try XCTUnwrap(env["LUMIA_UITEST_LAST"])
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        var app = launch(screen: "server")
        var anything: XCUIElementQuery { app.descendants(matching: .any) }
        func labelled(_ words: String) -> XCUIElement { anything.matching(NSPredicate(format: "label == %@", words)).firstMatch }
        func row(of who: String, saying words: String) -> XCUIElement {
            anything.matching(NSPredicate(format: "label BEGINSWITH %@ AND value == %@", "\(who), ", words)).firstMatch
        }
        var field: XCUIElement { composerField(in: app) }
        var general: XCUIElement { app.buttons["general"] }
        /// The server DM's row in the sidebar, named for the other person.
        var direct: XCUIElement { app.buttons.matching(NSPredicate(format: "label CONTAINS %@", other)).firstMatch }
        var replyBar: XCUIElement { labelled("Replying to \(other)") }
        var mention: XCUIElement { app.buttons["Mention \(other)"] }
        /// What the composer holds: an empty field reports its placeholder, "Message #general", as its value.
        func held() -> String {
            let value = field.value as? String ?? ""
            return value.hasPrefix("Message ") ? "" : value
        }
        func expectHeld(_ words: String, _ why: String) {
            for _ in 0..<50 where held() != words { usleep(200_000) }
            XCTAssertEqual(held(), words, why)
        }
        func sidebar() {
            let back = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch
            while back.exists { back.tap(); sleep(1) }
            XCTAssertTrue(general.waitForExistence(timeout: 10), "not back at the server's sidebar")
        }
        func open(_ conversation: XCUIElement) {
            sidebar()
            conversation.tap()
            XCTAssertTrue(composerField(in: app, within: 20).exists, "no composer")
        }
        /// Puts the caret after the field's last word.
        func tapEnd() { field.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap() }

        // #general, nothing kept yet: a reply begun to the other person's
        // message, its "@ On" turned off, and words typed and not sent.
        XCTAssertTrue(general.waitForExistence(timeout: 90), "no #general in the sidebar")
        open(general)
        expectHeld("", "a conversation nobody has typed in opens with words in its field")
        let theirs = row(of: other, saying: asked)
        XCTAssertTrue(theirs.waitForExistence(timeout: 30), "no message of \(other)'s in #general")
        theirs.press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["Reply"].waitForExistence(timeout: 10), "the message's actions have no Reply")
        app.buttons["Reply"].tap()
        XCTAssertTrue(replyBar.waitForExistence(timeout: 10), "no reply bar in the composer")
        XCTAssertEqual(mention.value as? String, "On", "@ On is not on for a new reply")
        mention.tap()
        if app.buttons["Continue"].exists { app.buttons["Continue"].tap() }
        field.typeText("Half a thought")
        expectHeld("Half a thought", "the composer does not hold what was typed")

        // The server DM has none of it, and takes words of its own.
        open(direct)
        expectHeld("", "#general's words followed into another conversation")
        XCTAssertFalse(app.buttons["Cancel reply"].exists, "#general's reply followed into another conversation")
        field.tap()
        field.typeText("Elsewhere")

        // Back in #general: the words, the reply and its "@ On" as they were left.
        open(general)
        expectHeld("Half a thought", "#general did not keep what was typed in it through a switch to another conversation")
        XCTAssertTrue(replyBar.waitForExistence(timeout: 10), "#general did not keep the reply being written")
        XCTAssertEqual(mention.value as? String, "Off", "the kept reply's @ On is on again")
        reached("switched")

        // A message edited in place is typed in a field of its own: the
        // draft is as it was while it is typed and once it is saved, here and
        // after leaving and returning.
        let mine = row(of: me, saying: "Mine to edit.")
        XCTAssertTrue(mine.waitForExistence(timeout: 10), "no message of this account's in #general")
        mine.press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["Edit message"].waitForExistence(timeout: 10), "one's own message's actions have no Edit message")
        app.buttons["Edit message"].tap()
        let editor = anything.matching(identifier: "message-editor").firstMatch
        XCTAssertTrue(editor.waitForExistence(timeout: 10), "Edit message put no field where the words were")
        editor.typeText(" Edited.")
        sleep(1)
        expectHeld("Half a thought", "the composer took an edit's words")
        editor.typeText("\n")
        XCTAssertTrue(row(of: me, saying: "Mine to edit. Edited., edited").waitForExistence(timeout: 20), "the edit did not come back as the message's words")
        open(general)
        expectHeld("Half a thought", "an edit of a message changed the conversation's draft")
        XCTAssertTrue(replyBar.exists, "an edit of a message took the draft's reply")

        // More typed and left to rest; then the smoke kills the app where it
        // stands, as a crash would: nothing is told that it is leaving.
        tapEnd()
        field.typeText(" and more")
        expectHeld("Half a thought and more", "the composer does not hold what was typed")
        sleep(2)
        reached("typed")
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 20), "the smoke did not end the app")

        // Launched again: each conversation has its own words, and #general its reply.
        app = launch(screen: "server")
        XCTAssertTrue(general.waitForExistence(timeout: 90), "no #general in the sidebar after the launch")
        open(general)
        expectHeld("Half a thought and more", "what was typed in #general was not there at the next launch")
        XCTAssertTrue(replyBar.waitForExistence(timeout: 10), "the reply being written was not there at the next launch")
        XCTAssertEqual(mention.value as? String, "Off", "the kept reply's @ On is on again at the next launch")
        reached("relaunched")
        open(direct)
        expectHeld("Elsewhere", "what was typed in the server DM was not there at the next launch")
        XCTAssertFalse(app.buttons["Cancel reply"].exists, "the server DM has #general's reply at the next launch")

        // Sent: the field is empty, and stays empty once a save that waited
        // would have run, through leaving and returning and another launch.
        open(general)
        expectHeld("Half a thought and more", "#general lost its words while the server DM was on screen")
        app.buttons["Send"].tap()
        XCTAssertTrue(row(of: me, saying: "Half a thought and more").waitForExistence(timeout: 20), "the message is not drawn")
        expectHeld("", "the field still holds what was sent")
        XCTAssertFalse(app.buttons["Cancel reply"].exists, "the reply bar is still in the composer after the send")
        sleep(2)
        open(general)
        expectHeld("", "sent words came back to the field after leaving and returning")
        XCTAssertFalse(app.buttons["Cancel reply"].exists, "a sent reply came back to the composer after leaving and returning")
        reached("sent")
        app.terminate()
        app = launch(screen: "server")
        XCTAssertTrue(general.waitForExistence(timeout: 90), "no #general in the sidebar after the second launch")
        open(general)
        expectHeld("", "sent words came back to the field at the next launch")
        XCTAssertFalse(app.buttons["Cancel reply"].exists, "a sent reply came back to the composer at the next launch")

        // A field emptied by hand keeps nothing.
        open(direct)
        expectHeld("Elsewhere", "the server DM lost its words to #general's send")
        tapEnd()
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: "Elsewhere".count))
        expectHeld("", "the field was not emptied")
        open(direct)
        expectHeld("", "words taken out of the field came back after leaving and returning")
        reached("emptied")
    }

    /// The same in a personal conversation (scripts/smoke-drafts.sh, after
    /// the test above on the same phone, which set private messages up as
    /// it signed in). LUMIA_UITEST_OTHER has asked for a conversation:
    /// accepted from Requests, it opens, and the smoke has them write
    /// LUMIA_UITEST_LAST in it (`accepted`). A reply to that is begun and
    /// words typed and left to rest; the smoke kills the app (`typed`);
    /// launched again on the conversation, the words are there and the
    /// reply bar names the same person, found from the message the kept
    /// reply answers. Sent, the field is empty and stays empty at another
    /// launch (`sent`, where the smoke reads the conversation as the other
    /// person).
    func testAPersonalConversationKeepsItsDraft() throws {
        let env = ProcessInfo.processInfo.environment
        let asked = try XCTUnwrap(env["LUMIA_UITEST_LAST"])
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        var app = launch(screen: "requests")
        var anything: XCUIElementQuery { app.descendants(matching: .any) }
        var field: XCUIElement { composerField(in: app) }
        var replyBar: XCUIElement { anything.matching(NSPredicate(format: "label BEGINSWITH %@", "Replying to ")).firstMatch }
        func held() -> String {
            let value = field.value as? String ?? ""
            return value.hasPrefix("Message ") ? "" : value
        }
        func expectHeld(_ words: String, _ why: String) {
            for _ in 0..<50 where held() != words { usleep(200_000) }
            XCTAssertEqual(held(), words, why)
        }

        // Accepting opens the conversation, with this phone in its group.
        XCTAssertTrue(app.buttons["Accept"].waitForExistence(timeout: 90), "no request waiting")
        app.buttons["Accept"].tap()
        let opened = composerField(in: app, within: 60)
        XCTAssertTrue(opened.exists, "accepting opened no conversation")
        let ready = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == true"), object: opened)
        XCTAssertEqual(XCTWaiter().wait(for: [ready], timeout: 60), .completed, "the conversation's composer never came on")
        expectHeld("", "a conversation nobody has typed in opens with words in its field")
        reached("accepted")

        // The other person's message: a reply begun, and words typed and left to rest.
        let theirs = anything.matching(NSPredicate(format: "value == %@", asked)).firstMatch
        XCTAssertTrue(theirs.waitForExistence(timeout: 60), "the other person's message is not drawn")
        theirs.press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["Reply"].waitForExistence(timeout: 10), "the message's actions have no Reply")
        app.buttons["Reply"].tap()
        XCTAssertTrue(replyBar.waitForExistence(timeout: 10), "no reply bar in the composer")
        let named = replyBar.label
        XCTAssertNotEqual(named, "Replying to a message", "the reply bar does not say whose message it answers")
        if app.buttons["Continue"].exists { app.buttons["Continue"].tap() }
        field.typeText("Thursday works")
        expectHeld("Thursday works", "the composer does not hold what was typed")
        sleep(2)
        reached("typed")
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 20), "the smoke did not end the app")

        // Launched again on the conversation: the words, and the reply bar
        // naming the same person once the conversation is read.
        app = launch(screen: "conversation")
        XCTAssertTrue(composerField(in: app, within: 90).exists, "no conversation after the launch")
        expectHeld("Thursday works", "what was typed in the conversation was not there at the next launch")
        XCTAssertTrue(replyBar.waitForExistence(timeout: 10), "the reply being written was not there at the next launch")
        for _ in 0..<50 where replyBar.label != named { usleep(200_000) }
        XCTAssertEqual(replyBar.label, named, "the kept reply does not say whose message it answers")
        reached("relaunched")

        // Sent: empty, and still empty at another launch.
        app.buttons["Send"].tap()
        XCTAssertTrue(anything.matching(NSPredicate(format: "value == %@", "Thursday works")).firstMatch.waitForExistence(timeout: 20), "the message is not drawn")
        expectHeld("", "the field still holds what was sent")
        XCTAssertFalse(app.buttons["Cancel reply"].exists, "the reply bar is still in the composer after the send")
        sleep(2)
        app.terminate()
        app = launch(screen: "conversation")
        XCTAssertTrue(composerField(in: app, within: 90).exists, "no conversation after the second launch")
        XCTAssertTrue(anything.matching(NSPredicate(format: "value == %@", "Thursday works")).firstMatch.waitForExistence(timeout: 30), "the sent message is not drawn after the launch")
        expectHeld("", "sent words came back to the field at the next launch")
        XCTAssertFalse(app.buttons["Cancel reply"].exists, "a sent reply came back to the composer at the next launch")
        reached("sent")
    }

    // MARK: Setting up private messages (issue #20; scripts/smoke-private-messages.sh)

    /// A phone signed in with a password to an account with no identity
    /// sets private messages up silently; Home reminds of the phrase until
    /// "In a week", which holds at the next launch; Devices' card leads to
    /// the phrase (the system's unlock skipped, LUMIA_DEV_NO_UNLOCK),
    /// whose words are read as a person would; a wrong word in the check
    /// shows the phrase again, saying so, and the right three end the card
    /// and the reminder.
    func testPrivateMessagesAreSetUpAndThePhraseConfirmed() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        var app = launch(screen: "home")
        let reminder = app.staticTexts["Save your recovery phrase"]
        XCTAssertTrue(reminder.waitForExistence(timeout: 90), "no reminder on Home: private messages were not set up at sign-in")
        XCTAssertTrue(app.buttons["Save it now"].exists)
        reached("reminder")
        app.buttons["In a week"].tap()
        XCTAssertFalse(reminder.waitForExistence(timeout: 3), "the reminder stayed after In a week")
        app.terminate()

        // The next launch: the reminder is still put off, and Devices' card is not.
        app = launch(screen: "devices")
        let card = app.staticTexts["Recovery phrase not saved"]
        XCTAssertTrue(card.waitForExistence(timeout: 90), "no card on Devices while the phrase waits")
        reached("devices")
        app.buttons["Save your recovery phrase"].tap()

        /// The 24 words, as their elements say them.
        func words() -> [String] {
            (1...24).map { n in
                let cell = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH %@", "Word \(n): ")).firstMatch
                if !cell.waitForExistence(timeout: 20) {
                    XCTFail("no word \(n), on:\n" + app.debugDescription)
                    return ""
                }
                return String(cell.label.dropFirst("Word \(n): ".count))
            }
        }
        let phrase = words()
        XCTAssertGreaterThan(Set(phrase).count, 1, "the words are not a phrase: \(phrase)")
        XCTAssertTrue(app.staticTexts["Whoever has these 24 words has your identity and your private messages. Lumia can't recover them for you."].exists)
        XCTAssertTrue(app.buttons["Print"].exists && app.buttons["Save as a file"].exists)
        XCTAssertFalse(app.buttons["Copy"].exists, "a copy button on the phrase")
        reached("phrase")

        /// Fills the check's three fields, the last one wrong when asked, and confirms.
        func answer(wrong: Bool) {
            app.buttons["I've written them down"].tap()
            let fields = app.textFields.matching(NSPredicate(format: "label BEGINSWITH %@", "Word "))
            let three = XCTNSPredicateExpectation(predicate: NSPredicate(format: "count == 3"), object: fields)
            XCTAssertEqual(XCTWaiter().wait(for: [three], timeout: 20), .completed, "not three fields to check")
            for i in 0..<3 {
                let field = fields.element(boundBy: i)
                guard let position = Int(field.label.dropFirst("Word ".count)) else { return XCTFail("a field named \(field.label)") }
                let right = phrase[position - 1]
                field.tap()
                field.typeText(wrong && i == 2 ? (right == "abandon" ? "zoo" : "abandon") : right)
            }
            if !wrong { reached("check") }
            app.buttons["Confirm"].tap()
        }
        answer(wrong: true)
        let again = app.staticTexts["One of those words wasn't right. Check what you wrote down against the phrase, then try again."]
        XCTAssertTrue(again.waitForExistence(timeout: 20), "a wrong word did not show the phrase again")
        XCTAssertEqual(words(), phrase, "the phrase shown again is another")
        reached("again")
        answer(wrong: false)

        XCTAssertTrue(app.staticTexts["This iPhone"].waitForExistence(timeout: 20), "not back on Devices with this phone listed")
        XCTAssertFalse(card.waitForExistence(timeout: 3), "the card stayed once the phrase was confirmed")
        reached("saved")
        app.buttons["Back to Home"].tap()
        XCTAssertFalse(reminder.waitForExistence(timeout: 3), "the reminder came back once the phrase was confirmed")
    }

    /// A phone signed in to an account whose identity is on another device
    /// (the CLI's): Home says private messages need this phone, its Set up
    /// opens the sheet, Restore asks first whether a device is still there
    /// and, told no, takes the 24 words and turns on at 24; the CLI's device
    /// is offered to keep, none ticked, and kept, and the phone is then the
    /// identity's device beside it, with nothing more to set up.
    func testThisPhoneIsBroughtInByRestoring() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let phrase = try XCTUnwrap(env["LUMIA_UITEST_PHRASE"]).split(separator: " ")
        let reached = { (step: String) in self.reached(step, under: steps) }
        let app = launch(screen: "home")
        let notice = app.staticTexts["Private messages need this phone. Your account has them on another device."]
        XCTAssertTrue(notice.waitForExistence(timeout: 90), "Home does not say private messages need this phone")
        XCTAssertFalse(app.staticTexts["Save your recovery phrase"].exists, "a reminder for a phrase this phone does not keep")
        app.buttons["Set up"].tap()
        XCTAssertTrue(app.staticTexts["Private messages need this phone"].waitForExistence(timeout: 10), "no sheet")
        for label in ["Link from a device you use", "Restore with your recovery phrase", "Lost every device and the phrase? Start over"] {
            XCTAssertTrue(app.buttons[label].exists, "the sheet has no \(label)")
        }
        reached("sheet")
        app.buttons["Restore with your recovery phrase"].tap()
        XCTAssertTrue(app.staticTexts["Do you still have a device with your private messages?"].waitForExistence(timeout: 10), "Restore does not ask first")
        XCTAssertTrue(app.buttons["Yes, link from it"].exists)
        reached("ask")
        app.buttons["No, restore with my phrase"].tap()
        let words = app.textViews["Your 24 words, in order"]
        XCTAssertTrue(words.waitForExistence(timeout: 10), "no field for the words")
        let restore = app.buttons["Restore"]
        XCTAssertFalse(restore.isEnabled, "Restore on with no words")
        words.tap()
        words.typeText(phrase.prefix(12).joined(separator: " "))
        XCTAssertFalse(restore.isEnabled, "Restore on at 12 words")
        XCTAssertTrue(app.staticTexts["12 of 24"].exists || (words.value as? String) == "12 of 24", "the count does not say 12 of 24")
        words.typeText(" " + phrase.dropFirst(12).joined(separator: " "))
        XCTAssertTrue(restore.isEnabled, "Restore off at 24 words")
        reached("restore")
        restore.tap()
        XCTAssertTrue(app.staticTexts["Keep any devices?"].waitForExistence(timeout: 30), "the CLI's device is not offered to keep")
        let device = app.switches.firstMatch
        XCTAssertTrue(device.exists, "no device to tick, on:\n" + app.debugDescription)
        XCTAssertEqual(device.value as? String, "0", "a device ticked before the person chose")
        XCTAssertTrue(app.buttons["Restore and stop them all"].exists)
        device.tap()
        XCTAssertEqual(device.value as? String, "1", "a tap does not tick it")
        let keep = app.buttons["Restore and keep 1 device"]
        XCTAssertTrue(keep.exists, "the button does not say one device is kept")
        reached("keep")
        keep.tap()
        XCTAssertTrue(app.staticTexts["Your conversations"].waitForExistence(timeout: 30), "not back on Home, set up")
        XCTAssertFalse(notice.exists, "Home still says private messages need this phone")
        reached("restored")
        app.buttons["Devices"].tap()
        XCTAssertTrue(app.staticTexts["2 linked to you"].waitForExistence(timeout: 10), "the CLI's device was not kept beside this phone")
        reached("kept")
    }

    /// Start over, for someone with neither device nor phrase: what stays
    /// and what does not, nobody to see a key change on an account with no
    /// personal conversation, the password, the danger's button; then a new
    /// phrase to save, which Home reminds of.
    func testStartingOverWithANewIdentity() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let password = try XCTUnwrap(env["LUMIA_DEV_PASSWORD"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        let app = launch(screen: "startover")
        XCTAssertTrue(app.staticTexts["Nobody has talked with you privately yet, so nobody will see a key change."].waitForExistence(timeout: 90), "no Start over, or the wrong words for who sees the key change")
        XCTAssertTrue(app.staticTexts["Private messages from before stay unreadable."].exists)
        let start = app.buttons["Start over with a new identity"]
        XCTAssertFalse(start.isEnabled, "Start over on with no password")
        let field = app.secureTextFields["Your password"]
        field.tap()
        field.typeText(password)
        XCTAssertTrue(start.isEnabled)
        reached("startover")
        start.tap()
        XCTAssertTrue(app.staticTexts["Save your recovery phrase"].waitForExistence(timeout: 30), "no new phrase to save after starting over")
        reached("started")
    }

    // MARK: Approving a device (identity spec 4.2; scripts/smoke-approve.sh)

    /// A phone that set private messages up at sign-in approves the CLI as
    /// a new device: Devices' Approve a device opens the screen, text that
    /// is not a request says so, the CLI's request through the system's
    /// Paste says what it would add and warns, Approve (the system's unlock
    /// skipped, LUMIA_DEV_NO_UNLOCK) shows the code, which the test leaves
    /// in LUMIA_UITEST_CODE for the smoke's CLI to claim, and Done is back
    /// on Devices with the new device listed.
    func testApprovingADeviceFromThisPhone() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let request = try XCTUnwrap(env["LUMIA_UITEST_REQUEST"])
        let label = try XCTUnwrap(env["LUMIA_UITEST_LABEL"])
        let codeFile = try XCTUnwrap(env["LUMIA_UITEST_CODE"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        let app = launch(screen: "devices")
        let open = app.buttons.matching(NSPredicate(format: "label == %@", "Approve a device")).firstMatch
        XCTAssertTrue(open.waitForExistence(timeout: 90), "no Approve a device on Devices: private messages were not set up at sign-in")
        reached("devices")
        open.tap()

        let field = app.textViews["Link request"]
        XCTAssertTrue(field.waitForExistence(timeout: 10), "no field for the request")
        let approve = app.buttons["Approve"]
        XCTAssertFalse(approve.isEnabled, "Approve on with nothing pasted")
        field.tap()
        field.typeText("hello")
        XCTAssertTrue(app.staticTexts["That isn't a link request."].waitForExistence(timeout: 5), "text that is not a request is not said to be one")
        XCTAssertFalse(approve.isEnabled, "Approve on for text that is not a request")
        reached("notarequest")
        app.buttons["Clear"].tap()

        // As a request shared from another device arrives: on the clipboard.
        UIPasteboard.general.string = request
        app.buttons["Paste"].tap()
        let adds = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH %@", "A device named “\(label)”")).firstMatch
        XCTAssertTrue(adds.waitForExistence(timeout: 10), "the pasted request does not say what it would add, on:\n" + app.debugDescription)
        XCTAssertTrue(adds.label.contains("It will read your private messages from now on."), "no warning: \(adds.label)")
        XCTAssertTrue(approve.isEnabled, "Approve off for a request")
        reached("request")
        approve.tap()

        let code = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Code")).firstMatch
        XCTAssertTrue(code.waitForExistence(timeout: 30), "no code after Approve, on:\n" + app.debugDescription)
        let value = (code.value as? String) ?? ""
        XCTAssertEqual(value.count, 8, "the code is not 8 characters: \(value)")
        XCTAssertTrue(app.staticTexts["Type this code on the new device. It works once, within ten minutes."].exists)
        try value.write(toFile: codeFile, atomically: true, encoding: .utf8)
        reached("code")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.staticTexts[label].waitForExistence(timeout: 10), "Devices does not list \(label) once approved")
        reached("listed")
    }

    /// Linking by a prompt, answered on this phone (scripts/smoke-link-prompt.sh,
    /// identity spec 4.2.1): once private messages are set up here it says
    /// so in LUMIA_UITEST_READY, and the script opens the link screen on a
    /// second simulator signed in to the same account. The prompt comes
    /// over Devices with no tap here, Continue shows the digits, which the
    /// test leaves in LUMIA_UITEST_DIGITS for the script to set beside the
    /// other phone's, "They match" (the unlock skipped) says the device is
    /// linked, and the prompt goes with Devices counting two.
    func testAPromptIsAnsweredOnThisPhone() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let label = try XCTUnwrap(env["LUMIA_UITEST_LABEL"])
        let ready = try XCTUnwrap(env["LUMIA_UITEST_READY"])
        let digitsFile = try XCTUnwrap(env["LUMIA_UITEST_DIGITS"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        let app = launch(screen: "devices")
        let set = app.buttons.matching(NSPredicate(format: "label == %@", "Approve a device")).firstMatch
        XCTAssertTrue(set.waitForExistence(timeout: 90), "private messages were not set up at sign-in")
        try "ready".write(toFile: ready, atomically: true, encoding: .utf8)

        let asked = app.staticTexts["\(label) wants your private messages"]
        XCTAssertTrue(asked.waitForExistence(timeout: 90), "no prompt, on:\n" + app.debugDescription)
        XCTAssertTrue(app.staticTexts["Only continue if you're linking a device of yours right now."].exists)
        XCTAssertTrue(app.buttons["Not me"].exists)
        reached("prompt")
        app.buttons["Continue"].tap()

        XCTAssertTrue(app.staticTexts["Does \(label) show this number?"].waitForExistence(timeout: 30), "no digits, on:\n" + app.debugDescription)
        let number = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Number")).firstMatch
        XCTAssertTrue(number.exists)
        let digits = (number.value as? String) ?? ""
        XCTAssertNotNil(digits.range(of: #"^\d{3} \d{3}$"#, options: .regularExpression), "not six digits in threes: \(digits)")
        try digits.write(toFile: digitsFile, atomically: true, encoding: .utf8)
        XCTAssertTrue(app.buttons["They don't match"].exists)
        reached("compare")
        app.buttons["They match"].tap()

        XCTAssertTrue(app.staticTexts["\(label) is linked."].waitForExistence(timeout: 30), "not linked, on:\n" + app.debugDescription)
        reached("linked")
        let gone = NSPredicate(format: "exists == false")
        expectation(for: gone, evaluatedWith: app.staticTexts["\(label) is linked."])
        waitForExpectations(timeout: 10)
        XCTAssertTrue(app.staticTexts["2 linked to you"].waitForExistence(timeout: 10), "Devices does not count the new device")
        reached("listed")
    }

    // MARK: Attachments and link cards (specs/media.md; scripts/smoke-media.sh)

    /// A channel where someone sent a photo, a PDF and a link with a card
    /// draws each as an element of its own that says what it is ("Photo,
    /// ridge.jpg", "File, notes.pdf, …", "Link, <host>, …"); the attach
    /// button opens the system's photo picker, the picture the smoke put in
    /// the library is chosen and goes into the strip, uploads, and is sent
    /// with words; the phone draws it in its message. Held, the picture,
    /// the card and the PDF each open their message's actions and nothing
    /// else; tapped, the picture opens full screen with Save to Photos,
    /// Share and Close, where two taps zoom it, and the PDF the share
    /// sheet. Steps under LUMIA_UITEST_STEPS: `drawn`, `picker`, `strip`,
    /// `sent`, `held`, `viewer`, `zoomed`, `share`.
    func testAttachingAPictureFromThePhotoLibrary() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        let carded = (env["LUMIA_UITEST_CARD"].flatMap(Int.init) ?? 0) > 0
        let app = launch()
        let anything = app.descendants(matching: .any)
        let labelled = { (prefix: String) in anything.matching(NSPredicate(format: "label BEGINSWITH %@", prefix)) }

        let photo = labelled("Photo, ridge.jpg").firstMatch
        if !photo.waitForExistence(timeout: 90) { print("the channel without the photo:\n" + app.debugDescription) }
        XCTAssertTrue(photo.exists, "the photo is not an element that says what it is")
        XCTAssertTrue(labelled("File, notes.pdf, ").firstMatch.waitForExistence(timeout: 10), "the PDF is not a file row that says its name and size")
        if carded { XCTAssertTrue(labelled("Link, ").firstMatch.waitForExistence(timeout: 20), "the link has no card") }
        reached("drawn")

        // The attach button's menu, then the system's photo picker.
        let attach = anything["attach"]
        XCTAssertTrue(attach.waitForExistence(timeout: 10), "no attach button: the instance's upload limits were not read")
        attach.tap()
        let photos = app.buttons["Photos and videos"]
        XCTAssertTrue(photos.waitForExistence(timeout: 10), "the attach menu has no Photos and videos")
        XCTAssertTrue(app.buttons["Files"].exists, "the attach menu has no Files")
        photos.tap()
        // The picker's newest picture, the one the smoke added.
        let picked = app.images.matching(NSPredicate(format: "label BEGINSWITH %@", "Photo")).firstMatch
        if !picked.waitForExistence(timeout: 30) { print("the picker without a photo:\n" + app.debugDescription) }
        XCTAssertTrue(picked.exists, "the photo picker shows no picture")
        reached("picker")
        picked.tap()
        let add = app.buttons["Add"]
        if add.waitForExistence(timeout: 5) { add.tap() }

        // In the strip: uploading, then its size; Send waits until then.
        let strip = labelled("lake").firstMatch
        let anyStrip = anything.matching(NSPredicate(format: "label CONTAINS %@ OR label CONTAINS %@", "Uploading…", " KB")).firstMatch
        XCTAssertTrue(strip.waitForExistence(timeout: 30) || anyStrip.waitForExistence(timeout: 1), "the picture is not in the strip")
        let uploaded = XCTNSPredicateExpectation(predicate: NSPredicate(format: "count == 0"), object: anything.matching(NSPredicate(format: "label CONTAINS %@", "Uploading…")))
        XCTAssertEqual(XCTWaiter().wait(for: [uploaded], timeout: 60), .completed, "the picture never finished uploading")
        XCTAssertFalse(anything.matching(NSPredicate(format: "label CONTAINS %@", "Too large")).firstMatch.exists)
        reached("strip")

        let field = anything.matching(NSPredicate(format: "placeholderValue BEGINSWITH %@", "Message #")).firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10), "no composer")
        field.tap()
        field.typeText("From the phone.")
        app.buttons["Send"].tap()
        // The phone's own picture in its message: a photo that is not the one it was sent.
        let mine = anything.matching(NSPredicate(format: "identifier == 'attachment' AND label BEGINSWITH 'Photo, ' AND NOT (label BEGINSWITH 'Photo, ridge')")).firstMatch
        if !mine.waitForExistence(timeout: 60) { print("the channel without the phone's picture:\n" + app.debugDescription) }
        XCTAssertTrue(mine.exists, "the phone's picture is not drawn in its message")
        let label = mine.label
        reached("sent")

        // A long press on the picture is its message's actions, as on its
        // words (Edit message: this account's own), and the viewer does not
        // open as well when the finger lifts.
        if app.keyboards.firstMatch.exists { app.swipeDown() }
        let viewer = app.buttons["Save to Photos"]
        mine.press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["action-reply"].waitForExistence(timeout: 10), "a long press on the picture did not open its message's actions")
        XCTAssertTrue(app.buttons["action-edit"].exists, "the actions a long press on the picture opened are not its message's")
        XCTAssertFalse(viewer.waitForExistence(timeout: 2), "a long press on the picture opened the viewer as well")
        reached("held")
        putAwaySheet(app, showing: app.buttons["action-reply"])
        XCTAssertFalse(viewer.exists, "the viewer opened once the actions went")

        // The picture full screen, with its ways out, and back.
        mine.tap()
        XCTAssertTrue(app.buttons["Save to Photos"].waitForExistence(timeout: 10), "the viewer has no Save to Photos")
        XCTAssertTrue(app.buttons["Share"].exists, "the viewer has no Share")
        XCTAssertTrue(anything.matching(NSPredicate(format: "label == %@", label)).firstMatch.waitForExistence(timeout: 20), "the viewer does not say what the picture is")
        reached("viewer")
        // Two taps in its middle zoom it (to its own size, a pixel a
        // point: scripts/smoke-viewer.sh measures that on a drawn picture;
        // this one is for the eye), and its ways out stay over it.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).doubleTap()
        sleep(1)
        XCTAssertTrue(app.buttons["Close"].isHittable, "the zoomed picture covers Close")
        reached("zoomed")
        app.buttons["Close"].tap()

        // Someone else's card, further up: their actions (no Edit message), and the link not opened.
        if carded {
            let card = labelled("Link, ").firstMatch
            for _ in 0..<6 where !(card.exists && card.isHittable) { app.scrollViews.firstMatch.swipeDown() }
            XCTAssertTrue(card.isHittable, "the card is not there to hold")
            card.press(forDuration: 1.0)
            XCTAssertTrue(app.buttons["action-reply"].waitForExistence(timeout: 10), "a long press on the card did not open its message's actions")
            XCTAssertFalse(app.buttons["action-edit"].exists, "the card's actions are not its message's")
            XCTAssertEqual(app.state, .runningForeground, "a long press on the card opened the link as well")
            putAwaySheet(app, showing: app.buttons["action-reply"])
        }

        // The PDF, further up: the share sheet, where it can be saved to Files.
        let file = labelled("File, notes.pdf, ").firstMatch
        for _ in 0..<6 where !(file.exists && file.isHittable) { app.scrollViews.firstMatch.swipeDown() }
        XCTAssertTrue(file.waitForExistence(timeout: 10), "the PDF's row is not there to open")
        let sheet = app.otherElements["ActivityListView"]
        let saveToFiles = app.buttons["Save to Files"]
        // Held, its message's actions and not the share sheet.
        file.press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["action-reply"].waitForExistence(timeout: 10), "a long press on the PDF did not open its message's actions")
        XCTAssertFalse(sheet.waitForExistence(timeout: 2) || saveToFiles.exists, "a long press on the PDF opened the share sheet as well")
        putAwaySheet(app, showing: app.buttons["action-reply"])
        file.tap()
        XCTAssertTrue(sheet.waitForExistence(timeout: 20) || saveToFiles.waitForExistence(timeout: 1), "the file did not open the share sheet")
        reached("share")
    }

    /// Inline video and embeds (scripts/smoke-media.sh, specs/media.md 5
    /// and 6.6), in a channel where someone sent a recording, links to a
    /// YouTube video, an X post and a Bluesky post named by its handle, and
    /// last a video. The video held is its message's actions and plays
    /// nothing; pressed, it plays in its row with the system's controls, and
    /// held while it plays it is still its message's actions. The YouTube
    /// card is a button named for what it plays; pressed, a web view takes
    /// its place. The Bluesky card is "Show post"; pressed, it is resolved
    /// and shown, and YouTube's goes back to its card; then X's. Then the
    /// recording plays in its row and X's post stays shown, being read and
    /// not played; a picture opened over the conversation and closed again
    /// leaves the recording playing, and so does scrolling away from it,
    /// past the post, which stays too. Last YouTube's card pressed again
    /// pauses the recording and puts X's post back to its card. The smoke
    /// looks at `video`, `youtube`, `bluesky`, `x` and `recording`; what
    /// each site drew is for the eye.
    func testVideoPlaysInPlaceAndEmbedsLoadOnAPress() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let reached = { (step: String) in self.reached(step, under: steps) }
        let app = launch()
        let anything = app.descendants(matching: .any)
        let labelled = { (prefix: String) in anything.matching(NSPredicate(format: "label BEGINSWITH %@", prefix)).firstMatch }
        let list = app.scrollViews.firstMatch
        // Scrolls toward something not yet on screen: up the conversation (earlier messages) or down it.
        let find = { (element: XCUIElement, earlier: Bool) -> Bool in
            for _ in 0..<10 where !(element.exists && element.isHittable) {
                let from = list.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: earlier ? 0.3 : 0.7))
                from.press(forDuration: 0.05, thenDragTo: list.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: earlier ? 0.7 : 0.3)))
            }
            let found = element.exists && element.isHittable
            // Settled, so a tap that follows is a tap and not the end of a fling.
            sleep(1)
            if !found {
                // What the screen held, for whoever reads the failure.
                try? XCUIScreen.main.screenshot().pngRepresentation.write(to: URL(fileURLWithPath: "\(steps)/not-found-\(Int(Date().timeIntervalSince1970)).png"))
                print("not found (exists: \(element.exists)):\n" + app.debugDescription)
            }
            return found
        }
        // A card pressed gives its place to its embed: its press's name goes, and a web view is there.
        let embeds = app.webViews
        let pressed = { (card: XCUIElement, name: String) in
            card.tap()
            let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: card)
            XCTAssertEqual(XCTWaiter().wait(for: [gone], timeout: 30), .completed, "pressing \(name)'s card did not load its embed in its place")
            XCTAssertTrue(embeds.firstMatch.waitForExistence(timeout: 10), "pressing \(name)'s card loaded no web view")
        }

        let clip = labelled("Video, clip.mp4")
        if !clip.waitForExistence(timeout: 90) { print("the channel without the video:\n" + app.debugDescription) }
        XCTAssertTrue(clip.exists, "the video is not an element that says what it is")
        let inline = anything["inline-video"]
        clip.press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["action-reply"].waitForExistence(timeout: 10), "a long press on the video's poster did not open its message's actions")
        putAwaySheet(app, showing: app.buttons["action-reply"])
        XCTAssertFalse(inline.exists, "the video played on a long press")
        clip.tap()
        if !inline.waitForExistence(timeout: 30) { print("the channel without the video playing:\n" + app.debugDescription) }
        XCTAssertTrue(inline.exists, "the video did not play in its row")
        XCTAssertFalse(app.buttons["Close"].exists, "the video opened full screen")
        sleep(2)
        reached("video")
        inline.press(forDuration: 1.0)
        XCTAssertTrue(app.buttons["action-reply"].waitForExistence(timeout: 10), "a long press on the playing video did not open its message's actions")
        putAwaySheet(app, showing: app.buttons["action-reply"])

        let youtube = labelled("Play video from YouTube, ")
        XCTAssertTrue(find(youtube, true), "the YouTube link's card has no play button named for what it plays")
        pressed(youtube, "YouTube")
        // The player whole on screen for the eye: a little further up when its top is under the header.
        let web = app.webViews.firstMatch
        for _ in 0..<4 where web.frame.minY < list.frame.minY + 40 {
            list.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.4)).press(forDuration: 0.05, thenDragTo: list.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.55)))
        }
        sleep(10)
        reached("youtube")

        let bluesky = labelled("Show post from Bluesky")
        XCTAssertTrue(find(bluesky, false), "the Bluesky link's card has no Show post")
        pressed(bluesky, "Bluesky")
        XCTAssertFalse(labelled("Bluesky couldn’t find").exists, "the Bluesky handle did not resolve")
        sleep(10)
        reached("bluesky")
        // A drag that starts on the post scrolls the conversation, not the post.
        let post = app.webViews.firstMatch
        let top = post.frame.minY
        post.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)).press(forDuration: 0.05, thenDragTo: post.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7)))
        XCTAssertNotEqual(post.frame.minY, top, "a drag over the Bluesky post did not scroll the conversation")
        XCTAssertTrue(find(youtube, true), "the YouTube embed did not go back to its card when another was shown")

        let x = labelled("Show post from X")
        XCTAssertTrue(find(x, false), "the X link's card has no Show post")
        pressed(x, "X")
        sleep(10)
        reached("x")

        // The recording at the top plays in its row, and X's post stays
        // shown: an attachment started leaves a post being read where it is.
        let play = anything.matching(NSPredicate(format: "label == 'Play' AND value == 'tone.m4a'")).firstMatch
        let pause = anything.matching(NSPredicate(format: "label == 'Pause' AND value == 'tone.m4a'")).firstMatch
        XCTAssertTrue(find(play, true), "the recording has no Play")
        play.tap()
        XCTAssertTrue(pause.waitForExistence(timeout: 20), "the recording did not play in its row")

        // A picture opened over the conversation is not the conversation
        // left: the recording plays on under the viewer.
        let photo = labelled("Photo, ridge.jpg")
        XCTAssertTrue(find(photo, false), "the photo under the recording is not there to open")
        photo.tap()
        XCTAssertTrue(app.buttons["Save to Photos"].waitForExistence(timeout: 10), "the photo did not open the viewer")
        sleep(2)
        app.buttons["Close"].tap()
        XCTAssertTrue(find(pause, true), "the recording stopped when a picture was opened over the conversation")

        // Scrolled away from, it plays on; and X's post, scrolled past
        // and with an attachment started since, is still shown.
        XCTAssertTrue(find(embeds.firstMatch, false), "the X post did not stay shown when the recording started and it was scrolled past")
        XCTAssertFalse(x.exists, "the X post went back to its card")
        reached("recording")
        XCTAssertTrue(find(pause, true), "the recording did not play on while its row was out of sight")

        // A card pressed stops everything else: the recording pauses, and
        // the post goes back to its card for the one embed shown.
        XCTAssertTrue(find(youtube, false), "the YouTube card is not there to press again")
        pressed(youtube, "YouTube")
        XCTAssertTrue(find(x, false), "the X post did not go back to its card when another card was pressed")
        XCTAssertTrue(find(play, true), "the recording did not pause when a card was pressed")
        XCTAssertFalse(pause.exists)
    }

    /// Images as links (scripts/smoke-media.sh launches in Minimal, specs/media.md
    /// 5): a recording is still a row that plays it in place on a press, and
    /// does not open the share sheet; a picture is a file's row.
    func testARecordingPlaysInItsRowWithImagesAsLinks() throws {
        let env = ProcessInfo.processInfo.environment
        let steps = try XCTUnwrap(env["LUMIA_UITEST_STEPS"])
        let app = launch()
        let anything = app.descendants(matching: .any)
        let list = app.scrollViews.firstMatch
        let play = anything.matching(NSPredicate(format: "label == 'Play' AND value == 'tone.m4a'")).firstMatch
        let row = anything.matching(NSPredicate(format: "label BEGINSWITH %@", "File, ridge.jpg")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 90) || play.exists, "the channel did not draw")
        for _ in 0..<10 where !(play.exists && play.isHittable) {
            list.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.3)).press(forDuration: 0.05, thenDragTo: list.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.7)))
        }
        if !play.exists { print("the channel in Minimal without the recording's Play:\n" + app.debugDescription) }
        XCTAssertTrue(play.exists && play.isHittable, "with Images as links the recording's row has no Play")
        XCTAssertTrue(row.exists, "with Images as links the photo is not a file's row")
        XCTAssertFalse(anything.matching(NSPredicate(format: "label BEGINSWITH %@", "Photo, ridge.jpg")).firstMatch.exists, "the photo is drawn with Images as links")
        sleep(1)
        play.tap()
        XCTAssertTrue(anything.matching(NSPredicate(format: "label == 'Pause' AND value == 'tone.m4a'")).firstMatch.waitForExistence(timeout: 20), "with Images as links the recording did not play from its row")
        XCTAssertFalse(app.otherElements["ActivityListView"].waitForExistence(timeout: 2) || app.buttons["Save to Files"].exists, "the recording's row opened the share sheet")
        sleep(1)
        reached("links", under: steps)
    }

    /// The viewer's zoom by hand (scripts/smoke-viewer.sh, specs/media.md
    /// 5), on the pictures a debug build draws with no instance
    /// (`LUMIA_DEV_SCREEN` `viewer` and `viewer-small`). The large one is
    /// tapped twice at a point, which its first step says in points after
    /// the screen's size, and twice again there; the small one is tapped
    /// twice, pinched out as far as it goes and dragged to its leading
    /// edge. Where the screen turns (an iPad; the app on a phone stays
    /// upright), each is also turned while zoomed, and the large one
    /// tapped twice once turned. What the screen shows at each step is the
    /// script's to check in its screenshot. Steps under
    /// LUMIA_UITEST_STEPS: `fitted`, which also says whether the screen
    /// turns, `own`, `back`, then `turned` and `refitted`, each saying the
    /// turned screen's size, on a screen that turns; `small`, `twice`,
    /// `furthest`, `dragged`, then `small-turned`.
    func testTheViewerZoomsByTheCoresRules() throws {
        let steps = try XCTUnwrap(ProcessInfo.processInfo.environment["LUMIA_UITEST_STEPS"])
        let turns = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = .portrait
        var app = launch(screen: "viewer")
        var picture = app.images["Photo, squares"]
        XCTAssertTrue(picture.waitForExistence(timeout: 60), "the viewer's picture is not an image that says what it is")
        let room = picture.frame
        XCTAssertEqual(room, app.windows.firstMatch.frame, "the viewer's picture has the whole screen to zoom in")
        // Right of the middle and under it, inside one of the picture's squares.
        let at = CGPoint(x: (room.width * 0.56).rounded(), y: (room.height * 0.53).rounded())
        let point = picture.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: at.x, dy: at.y))
        sleep(1)
        reached("fitted", under: steps, saying: "\(Int(room.width)) \(Int(room.height)) \(Int(at.x)) \(Int(at.y)) \(turns ? "turns" : "upright")")
        point.doubleTap()
        sleep(1)
        reached("own", under: steps)
        point.doubleTap()
        sleep(1)
        reached("back", under: steps)
        /// Turns the screen on its side and says its size once it has turned.
        func turn() -> String {
            XCUIDevice.shared.orientation = .landscapeLeft
            sleep(2)
            let turned = picture.frame
            XCTAssertEqual(turned.size, CGSize(width: room.height, height: room.width), "the viewer's picture did not turn with the screen")
            return "\(Int(turned.width)) \(Int(turned.height))"
        }
        if turns {
            // Zoomed to its own size again, then turned: the zoom is kept,
            // in times the size the picture is fitted at, which is another
            // on the turned screen; two taps then fit it there.
            point.doubleTap()
            sleep(1)
            let turned = turn()
            reached("turned", under: steps, saying: turned)
            picture.doubleTap()
            sleep(1)
            reached("refitted", under: steps, saying: turned)
            XCUIDevice.shared.orientation = .portrait
            sleep(2)
        }

        app = launch(screen: "viewer-small")
        picture = app.images["Photo, squares"]
        XCTAssertTrue(picture.waitForExistence(timeout: 60), "the small picture is not an image that says what it is")
        sleep(1)
        reached("small", under: steps)
        // Off its middle: twice its size still fits the screen, so it stays there.
        picture.coordinate(withNormalizedOffset: CGVector(dx: 0.6, dy: 0.52)).doubleTap()
        sleep(1)
        reached("twice", under: steps)
        // Far past four times its own size, which is where it stops.
        picture.pinch(withScale: 12, velocity: 6)
        sleep(2)
        reached("furthest", under: steps)
        // Wider than the screen now and not as tall: a drag across and
        // down stops at its leading edge and moves it no way down.
        picture.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 0.4)).press(forDuration: 0.05, thenDragTo: picture.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.7)))
        picture.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 0.4)).press(forDuration: 0.05, thenDragTo: picture.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.7)))
        sleep(2)
        reached("dragged", under: steps)
        if turns {
            // Four times its own size fits a screen this large either way
            // up: it stays that size, in the turned screen's middle.
            reached("small-turned", under: steps, saying: turn())
            XCUIDevice.shared.orientation = .portrait
        }
    }
}
