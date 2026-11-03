// The one thing the simulator cannot do from the command line: answer the
// notification permission prompt. scripts/smoke-push.sh runs this test to
// launch the app signed in (the LUMIA_DEV_* environment, passed through
// xcodebuild's TEST_RUNNER_ prefix), accept the prompt, and keep the app
// running while the link completes and the gateway publishes key packages.
import XCTest

final class SmokeUITests: XCTestCase {
    func testAllowNotificationsAndHold() {
        let app = XCUIApplication()
        for (key, value) in ProcessInfo.processInfo.environment where key.hasPrefix("LUMIA_DEV_") {
            app.launchEnvironment[key] = value
        }
        app.launch()
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let allow = springboard.buttons["Allow"]
        if allow.waitForExistence(timeout: 90) {
            allow.tap()
        }
        let hold = UInt32(ProcessInfo.processInfo.environment["LUMIA_UITEST_HOLD"] ?? "") ?? 20
        sleep(hold)
        XCTAssertEqual(app.state, .runningForeground)
    }
}
