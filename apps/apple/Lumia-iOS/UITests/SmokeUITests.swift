// The things the simulator cannot do from the command line: answer the
// notification permission prompt, tap a notification, type into a
// composer, and read a screen as assistive technology does. scripts/
// smoke-push.sh and scripts/smoke-typing.sh run these tests with the app
// signed in (the LUMIA_DEV_* environment, passed through xcodebuild's
// TEST_RUNNER_ prefix).
import XCTest

final class SmokeUITests: XCTestCase {
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        for (key, value) in ProcessInfo.processInfo.environment where key.hasPrefix("LUMIA_DEV_") {
            app.launchEnvironment[key] = value
        }
        app.launch()
        return app
    }

    /// Accepts the prompt and keeps the app running while the link
    /// completes and the gateway publishes key packages.
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
