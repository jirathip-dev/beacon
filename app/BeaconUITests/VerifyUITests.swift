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
        XCTAssert(app.staticTexts.matching(NSPredicate(format: "label MATCHES 'checked [0-9]{2}:[0-9]{2}'")).firstMatch.waitForExistence(timeout: 5))
        try shot("list")
        let unreviewed = app.staticTexts["com.example.sync"]
        for _ in 0..<5 where !unreviewed.isHittable { app.swipeUp() }
        XCTAssert(unreviewed.isHittable)
        try shot("unreviewed")
        app.buttons["[cfg]"].tap()
        XCTAssert(app.staticTexts["fetched"].waitForExistence(timeout: 5))
        try shot("settings")
    }

    /// The live `beacon serve` host's detail and its cpu chart on 1h and 24h, then a host with no `/metrics`.
    func testHostDetailAndChart() throws {
        launchOnFixtures()
        app.staticTexts["Live serve"].tap()
        XCTAssert(app.staticTexts["~ host"].waitForExistence(timeout: 10))
        try shot("host-detail")
        app.staticTexts["cpu"].tap()
        XCTAssert(app.buttons["[24h]"].waitForExistence(timeout: 5))
        try shot("chart-1h")
        app.buttons["[24h]"].tap()
        try shot("chart-24h")
        app.buttons["[done]"].tap()
        app.buttons["[‹ monitors]"].tap()
        app.staticTexts["Disk space"].tap()
        XCTAssert(app.staticTexts["dark lamp · no metrics"].waitForExistence(timeout: 10))
        try shot("host-not-served")
    }

    /// An http monitor's history on the `prober` (24h, 1h, 30d), then a check's history from its host detail.
    func testHistory() throws {
        launchOnFixtures()
        app.staticTexts["Status page"].tap()
        XCTAssert(app.staticTexts["~ response time"].waitForExistence(timeout: 10))
        try shot("monitor-history")
        app.buttons["[1h]"].tap()
        try shot("monitor-1h")
        XCTAssertEqual(app.staticTexts.matching(identifier: "max").count, 1)
        app.buttons["[30d]"].tap()
        XCTAssertEqual(app.staticTexts.matching(identifier: "max").count, 2)
        try shot("monitor-30d")
        app.buttons["[‹ monitors]"].tap()
        app.staticTexts["Nightly backup"].tap()
        XCTAssert(app.staticTexts["~ checks on macmini"].waitForExistence(timeout: 10))
        try shot("host-checks")
        app.staticTexts["Nightly backup"].tap()
        XCTAssert(app.staticTexts["~ incidents"].waitForExistence(timeout: 10))
        try shot("check-history")
    }

    /// Waits for `element`, answering the notification alert iOS may show over SpringBoard at any point.
    func appears(_ element: XCUIElement, timeout: TimeInterval = 30) -> Bool {
        let allow = springboard.alerts.buttons["Allow"]
        for _ in 0..<Int(timeout) {
            if allow.exists { allow.tap() }
            if element.waitForExistence(timeout: 1) && !allow.exists { return true }
        }
        return false
    }

    func tap(_ element: XCUIElement, timeout: TimeInterval = 30) {
        XCTAssert(appears(element, timeout: timeout), "never appeared: \(element)")
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
        XCTAssert(appears(icon, timeout: 60), "no Beacon icon on the home screen")
        let edit = springboard.buttons["Edit"]
        let menu = springboard.buttons["Edit Home Screen"]
        for _ in 0..<3 where !edit.exists {
            icon.press(forDuration: 1.5)
            if appears(menu, timeout: 15) { menu.tap() }
            _ = appears(edit)
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
            XCTAssert(appears(small, timeout: 10), "no size preview")
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
