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

    /// Waits for `element`, then taps it; fails with its query when it never shows.
    func tap(_ element: XCUIElement, timeout: TimeInterval = 30) {
        XCTAssert(element.waitForExistence(timeout: timeout), "never appeared: \(element)")
        element.tap()
    }

    func testWidgets() throws {
        launchOnFixtures()
        var home = false
        for _ in 0..<3 where !home {
            XCUIDevice.shared.press(.home)
            home = app.wait(for: .runningBackground, timeout: 5)
        }
        XCTAssert(home, "Beacon still in front after Home")
        let icon = springboard.icons["Beacon"]
        XCTAssert(icon.waitForExistence(timeout: 60), "no Beacon icon on the home screen")
        let edit = springboard.buttons["Edit"]
        let menu = springboard.buttons["Edit Home Screen"]
        for _ in 0..<3 where !edit.exists {
            icon.press(forDuration: 1.5)
            if menu.waitForExistence(timeout: 15) { menu.tap() }
            _ = edit.waitForExistence(timeout: 30)
        }
        XCTAssert(edit.exists, "home screen never entered edit mode: \(springboard.debugDescription)")
        let widgets = springboard.icons.matching(NSPredicate(format: "identifier == 'Beacon' AND value BEGINSWITH 'Widget'"))
        for medium in [false, true] {
            tap(edit)
            tap(springboard.buttons["Add Widget"])
            let search = springboard.searchFields.firstMatch
            tap(search)
            search.typeText("Beacon")
            tap(springboard.cells["Beacon"])
            let small = springboard.buttons.matching(NSPredicate(format: "value ENDSWITH 'Small'")).firstMatch
            XCTAssert(small.waitForExistence(timeout: 10), "no size preview")
            if medium {
                small.swipeLeft()
                XCTAssert(springboard.buttons.matching(NSPredicate(format: "value ENDSWITH 'Medium'")).firstMatch.waitForExistence(timeout: 5))
            }
            tap(springboard.buttons.matching(NSPredicate(format: "label ENDSWITH 'Add Widget'")).firstMatch)
            let placed = expectation(for: NSPredicate(format: "count == %d", medium ? 2 : 1), evaluatedWith: widgets)
            wait(for: [placed], timeout: 10)
        }
        tap(springboard.buttons["Done"])
        XCTAssert(edit.waitForNonExistence(timeout: 10))
        try shot("widgets")
        let placed = widgets.allElementsBoundByIndex.sorted { $0.frame.width < $1.frame.width }
        XCTAssertEqual(placed.count, 2)
        try shot("widget-small", placed[0].screenshot())
        try shot("widget-medium", placed[1].screenshot())
    }
}
