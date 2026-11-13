import SwiftUI

// Design harness only: picks one screen from the SCREEN env var (render.sh sets it) so each can be
// screenshotted in the simulator. Not product code; impl builds the approved look in app/Beacon.

enum Look { case a, b }

/// iPhone 390 pt widget sizes (HIG): small 158×158, medium 338×158.
enum WidgetSize {
    static let small = CGSize(width: 158, height: 158)
    static let medium = CGSize(width: 338, height: 158)
}

struct WidgetTile<Content: View>: View {
    let look: Look
    let size: CGSize
    @ViewBuilder let content: Content
    var body: some View {
        content
            .padding(16)
            .frame(width: size.width, height: size.height)
            .background(look == .a ? Color(.systemBackground) : B.bg)
            .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
    }
}

/// The three widget tiles: small with monitors down, small all up, medium with monitors down.
func widgetTiles(_ look: Look) -> [(String, AnyView)] {
    switch look {
    case .a:
        [("small", AnyView(WidgetTile(look: .a, size: WidgetSize.small) { ASmallWidget(state: Fixture.widgetDown) })),
         ("small-all-up", AnyView(WidgetTile(look: .a, size: WidgetSize.small) { ASmallWidget(state: Fixture.widgetUp) })),
         ("medium", AnyView(WidgetTile(look: .a, size: WidgetSize.medium) { AMediumWidget(state: Fixture.widgetDown) }))]
    case .b:
        [("small", AnyView(WidgetTile(look: .b, size: WidgetSize.small) { BSmallWidget(state: Fixture.widgetDown) })),
         ("small-all-up", AnyView(WidgetTile(look: .b, size: WidgetSize.small) { BSmallWidget(state: Fixture.widgetUp) })),
         ("medium", AnyView(WidgetTile(look: .b, size: WidgetSize.medium) { BMediumWidget(state: Fixture.widgetDown) }))]
    }
}

struct WidgetBoard: View {
    let look: Look
    var body: some View {
        VStack(spacing: 20) {
            ForEach(widgetTiles(look), id: \.0) { $0.1 }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(red: 0.35, green: 0.42, blue: 0.55))
    }
}

/// Writes each widget tile to $OUT/<look>-widget-<name>[-dark].png at 3×.
@MainActor func exportWidgets(look: Look, dark: Bool, to dir: String) {
    for (name, view) in widgetTiles(look) {
        let renderer = ImageRenderer(content: view.environment(\.colorScheme, dark ? .dark : .light))
        renderer.scale = 3
        let file = "\(look == .a ? "A" : "B")-widget-\(name)\(dark && look == .a ? "-dark" : "").png"
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
            screen(env["SCREEN"] ?? "A-list")
                .preferredColorScheme(env["DARK"] == "1" ? .dark : nil)
                .task {
                    guard let out = env["OUT"] else { return }
                    exportWidgets(look: .a, dark: false, to: out)
                    exportWidgets(look: .a, dark: true, to: out)
                    exportWidgets(look: .b, dark: true, to: out)
                }
        }
    }

    @ViewBuilder func screen(_ name: String) -> some View {
        switch name {
        case "A-first-run": AFirstRun()
        case "A-first-run-error": AFirstRun(error: Fixture.decodeError)
        case "A-list-offline": AList(state: Fixture.offline)
        case "A-settings": ASettings()
        case "A-widgets": WidgetBoard(look: .a)
        case "B-first-run": BFirstRun()
        case "B-first-run-error": BFirstRun(error: Fixture.decodeError)
        case "B-list": BList(state: Fixture.list)
        case "B-list-offline": BList(state: Fixture.offline)
        case "B-settings": BSettings()
        case "B-widgets": WidgetBoard(look: .b)
        default: AList(state: Fixture.list)
        }
    }
}
