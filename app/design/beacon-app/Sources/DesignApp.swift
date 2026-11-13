import SwiftUI

// Design harness only: picks one screen from the SCREEN env var (render.sh sets it) so each can be
// screenshotted in the simulator. Not product code; impl builds this look in app/Beacon.

/// iPhone 390 pt widget sizes (HIG): small 158×158, medium 338×158.
enum WidgetSize {
    static let small = CGSize(width: 158, height: 158)
    static let medium = CGSize(width: 338, height: 158)
}

struct WidgetTile<Content: View>: View {
    let size: CGSize
    @ViewBuilder let content: Content
    var body: some View {
        content
            .padding(16)
            .frame(width: size.width, height: size.height)
            .background(T.bg)
            .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
    }
}

/// The three widget tiles: small with monitors down, small all up, medium with monitors down.
func widgetTiles() -> [(String, AnyView)] {
    [("small", AnyView(WidgetTile(size: WidgetSize.small) { SmallWidget(state: Fixture.widgetDown) })),
     ("small-all-up", AnyView(WidgetTile(size: WidgetSize.small) { SmallWidget(state: Fixture.widgetUp) })),
     ("medium", AnyView(WidgetTile(size: WidgetSize.medium) { MediumWidget(state: Fixture.widgetDown) }))]
}

struct WidgetBoard: View {
    var body: some View {
        VStack(spacing: 20) {
            ForEach(widgetTiles(), id: \.0) { $0.1 }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(red: 0.35, green: 0.42, blue: 0.55))
    }
}

/// Writes each widget tile to $OUT/widget-<name>-<light|dark>.png at 3×.
@MainActor func exportWidgets(dark: Bool, to dir: String) {
    for (name, view) in widgetTiles() {
        let renderer = ImageRenderer(content: view.environment(\.colorScheme, dark ? .dark : .light))
        renderer.scale = 3
        let file = "widget-\(name)-\(dark ? "dark" : "light").png"
        if let data = renderer.uiImage?.pngData() {
            try? data.write(to: URL(fileURLWithPath: dir).appendingPathComponent(file))
        }
    }
}

@main
struct DesignApp: App {
    let env = ProcessInfo.processInfo.environment

    var body: some Scene {
        WindowGroup {
            screen(env["SCREEN"] ?? "list")
                .task {
                    guard let out = env["OUT"] else { return }
                    exportWidgets(dark: false, to: out)
                    exportWidgets(dark: true, to: out)
                }
        }
    }

    @ViewBuilder func screen(_ name: String) -> some View {
        switch name {
        case "first-run": FirstRun()
        case "first-run-error": FirstRun(error: Fixture.decodeError)
        case "list-offline": MonitorList(state: Fixture.offline)
        case "list-empty": MonitorList(state: Fixture.empty)
        case "list-loading": MonitorList(state: Fixture.loading)
        case "settings": Settings()
        case "widgets": WidgetBoard()
        default: MonitorList(state: Fixture.list)
        }
    }
}
