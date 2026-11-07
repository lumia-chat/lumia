// The things the simulator cannot do from the command line: answer the
// notification permission prompt, and tap a notification. scripts/
// smoke-push.sh runs these tests with the app signed in (the LUMIA_DEV_*
// environment, passed through xcodebuild's TEST_RUNNER_ prefix).
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
}
