// The things the simulator cannot do from the command line: answer the
// notification permission prompt, tap a notification, type into a
// composer, make a choice in Preferences, sign out from the account menu,
// look into Notification Center, and read a screen as assistive technology
// does. scripts/smoke-push.sh,
// scripts/smoke-typing.sh, scripts/smoke-preferences.sh,
// scripts/smoke-signout.sh and scripts/smoke-server-settings.sh run these tests with the app signed in
// (the LUMIA_DEV_* environment, passed through xcodebuild's TEST_RUNNER_
// prefix).
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
    /// says whether it is the one selected, a choice takes effect as it is
    /// made and is there at the next launch, which draws a channel by it.
    /// The smoke looks at the screen at each step announced under
    /// LUMIA_UITEST_STEPS: `menu`, `dark`, `kept` (a channel, dark and
    /// compact from what was stored alone) and `system`.
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
        /// One of a card's choices, inside the group that says what is being chosen.
        func choice(_ group: String, _ label: String) -> XCUIElement {
            app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", group)).buttons[label]
        }
        /// Which of a group's choices say they are selected.
        func selected(_ group: String, of labels: [String]) -> [String] {
            labels.filter { choice(group, $0).isSelected }
        }
        let appearances = ["Light", "Dark", "System"]
        let densities = ["Comfortable", "Compact"]

        openPreferences()
        _ = reached("menu")
        app.collectionViews.buttons["Preferences"].tap()
        for words in ["Preferences", "How Lumia looks on this phone", "Appearance", "Light, dark, or whichever this phone is in.", "Messages", "Comfortable gives each message room. Compact shows more of a conversation at once."] {
            XCTAssertTrue(app.staticTexts[words].waitForExistence(timeout: 10), "the screen does not say \"\(words)\"")
        }
        for label in appearances { XCTAssertTrue(choice("Appearance", label).exists, "no \(label) under Appearance") }
        for label in densities { XCTAssertTrue(choice("Message density", label).exists, "no \(label) under Message density") }
        XCTAssertFalse(app.staticTexts["Scroll bars"].exists, "a phone's scroll indicators are the system's")
        // As a new phone has them.
        XCTAssertEqual(selected("Appearance", of: appearances), ["System"])
        XCTAssertEqual(selected("Message density", of: densities), ["Comfortable"])

        choice("Appearance", "Dark").tap()
        XCTAssertEqual(selected("Appearance", of: appearances), ["Dark"])
        choice("Message density", "Compact").tap()
        XCTAssertEqual(selected("Message density", of: densities), ["Compact"])
        _ = reached("dark")

        // The next launch names neither choice: what it draws is what was kept.
        app.terminate()
        app = launch(screen: "channel")
        XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(format: "placeholderValue BEGINSWITH %@", "Message #")).firstMatch.waitForExistence(timeout: 90), "the channel did not open")
        _ = reached("kept")
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch.tap()
        openPreferences()
        app.collectionViews.buttons["Preferences"].tap()
        XCTAssertTrue(choice("Appearance", "Dark").waitForExistence(timeout: 10))
        XCTAssertEqual(selected("Appearance", of: appearances), ["Dark"])
        XCTAssertEqual(selected("Message density", of: densities), ["Compact"])

        // And back, as it is made: the phone's own appearance again.
        choice("Appearance", "System").tap()
        choice("Message density", "Comfortable").tap()
        XCTAssertEqual(selected("Appearance", of: appearances), ["System"])
        XCTAssertEqual(selected("Message density", of: densities), ["Comfortable"])
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
    /// Steps (each waits for `<step>.seen`): `open`, `warning`, `saved`
    /// (the smoke reads the server from the instance and sets the picture),
    /// `picture`, and `rail`, which carries where the server's tile is, as
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

        for words in ["Server settings", serverName, "Overview", "Choose a picture…", "Name", "Message history", "How long this server keeps its messages."] {
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

        // The smoke has given the server a picture from elsewhere; the
        // account event brings it here.
        app.swipeDown()
        let remove = app.buttons["Remove"]
        XCTAssertTrue(remove.waitForExistence(timeout: 30), "the picture set elsewhere did not reach the screen")
        XCTAssertTrue(anything.matching(NSPredicate(format: "label == %@", "Server picture")).firstMatch.exists, "the picture is not labelled")
        reached("picture", under: steps)

        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Back to ")).firstMatch.tap()
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

    /// Channel membership by hand (scripts/smoke-membership.sh, which seeds
    /// the server: an invitation into #design from Ada, #random to browse,
    /// Ada taking adds as invitations, Grace in #general). Each step is
    /// announced and waits for `<step>.seen`, the smoke reading the instance
    /// at it: `invitation` (joined #design from the sidebar's row),
    /// `browsed` (#random read from Browse, then joined from its bar),
    /// `added` (Grace added to #random and Ada invited, from Add people),
    /// `removed` (Grace removed by the row's swipe), `dm` (a server DM with
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
        XCTAssertTrue(app.textFields["Message #design"].waitForExistence(timeout: 20), "joining from the invitation did not open #design with its composer")
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
        XCTAssertFalse(app.textFields["Message #random"].exists, "a composer in a channel the account is not in")
        app.buttons["Join"].firstMatch.tap()
        XCTAssertTrue(app.textFields["Message #random"].waitForExistence(timeout: 20), "joining from the bar did not bring the composer back")
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

        // Removed by the row's swipe, as the owner may.
        graceRow.swipeLeft()
        app.buttons["Remove from #random"].tap()
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
        let composer = app.textFields["Message Ada Lovelace"]
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
        XCTAssertTrue(labelled("Default channels").waitForExistence(timeout: 20))
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

        // Leave #random from its members: back at the server, it is gone from the sidebar.
        app.buttons["random"].tap()
        app.buttons["Members"].tap()
        app.buttons["Leave channel"].tap()
        XCTAssertTrue(labelled("Direct messages").waitForExistence(timeout: 20), "leaving did not go back to the server")
        let left = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: app.buttons["random"])
        XCTAssertEqual(XCTWaiter().wait(for: [left], timeout: 20), .completed, "#random is still in the sidebar once left")
        reached("left", under: steps)
    }

    /// Announces a step to the smoke and waits until it says it has looked.
    private func reached(_ step: String, under steps: String, saying words: String? = nil) {
        try? (words ?? step).write(toFile: "\(steps)/\(step)", atomically: true, encoding: .utf8)
        for _ in 0..<1200 where !FileManager.default.fileExists(atPath: "\(steps)/\(step).seen") { usleep(100_000) }
        XCTAssertTrue(FileManager.default.fileExists(atPath: "\(steps)/\(step).seen"), "the smoke never looked at \"\(step)\"")
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
}
