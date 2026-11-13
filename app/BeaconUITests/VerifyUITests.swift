import XCTest

/// Drives the app for `.drover/verify.md`: each test writes `<screen>.png` into `$SHOTS`.
@MainActor final class VerifyUITests: XCTestCase {
    let env = ProcessInfo.processInfo.environment
    let app = XCUIApplication(bundleIdentifier: "dev.jirathip.beacon")
    let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")

    override func setUp() async throws { continueAfterFailure = false }

    func shot(_ name: String, _ image: XCUIScreenshot = XCUIScreen.main.screenshot()) throws {
        try image.pngRepresentation.write(to: URL(fileURLWithPath: env["SHOTS"]!).appending(path: "\(name).png"))
    }

    /// Launches with the fixture config served on `$PORT` as the source; waits for the first check to land.
    func launchOnFixtures() {
        let source = Data("{\"url\":{\"_0\":\"http://127.0.0.1:\(env["PORT"]!)/monitors.json\"}}".utf8)
        app.launchArguments = ["-source", "<\(source.map { String(format: "%02x", $0) }.joined())>"]
        app.launch()
        let allow = springboard.buttons["Allow"]
        if allow.waitForExistence(timeout: 3) { allow.tap() }
        XCTAssert(app.staticTexts["backup.ok is 31h old"].waitForExistence(timeout: 20))
    }

    func testFirstRun() throws {
        app.launchArguments = []
        app.launch()
        let allow = springboard.buttons["Allow"]
        if allow.waitForExistence(timeout: 3) { allow.tap() }
        XCTAssert(app.staticTexts["config url"].waitForExistence(timeout: 10))
        try shot("first-run")
    }

    func testListAndSettings() throws {
        launchOnFixtures()
        try shot("list")
        app.buttons["[cfg]"].tap()
        XCTAssert(app.staticTexts["fetched"].waitForExistence(timeout: 5))
        try shot("settings")
    }

    func testWidgets() throws {
        launchOnFixtures()
        XCUIDevice.shared.press(.home)
        springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.65)).press(forDuration: 2.5)
        for medium in [false, true] {
            springboard.buttons["Edit"].tap()
            springboard.buttons["Add Widget"].tap()
            let search = springboard.searchFields.firstMatch
            XCTAssert(search.waitForExistence(timeout: 5))
            search.tap()
            search.typeText("Beacon")
            springboard.cells["Beacon"].tap()
            if medium { springboard.buttons.matching(NSPredicate(format: "value ENDSWITH 'Small'")).firstMatch.swipeLeft() }
            springboard.buttons.matching(NSPredicate(format: "label ENDSWITH 'Add Widget'")).firstMatch.tap()
            sleep(2)
        }
        springboard.buttons["Done"].tap()
        sleep(3)
        try shot("widgets")
        let widgets = springboard.icons.matching(NSPredicate(format: "identifier == 'Beacon' AND value BEGINSWITH 'Widget'"))
            .allElementsBoundByIndex.sorted { $0.frame.width < $1.frame.width }
        XCTAssertEqual(widgets.count, 2)
        try shot("widget-small", widgets[0].screenshot())
        try shot("widget-medium", widgets[1].screenshot())
    }
}
