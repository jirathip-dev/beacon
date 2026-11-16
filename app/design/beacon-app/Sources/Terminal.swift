import SwiftUI
import UIKit

// The terminal look (owner pick, q1584), on Cooee's palette (cooee web/src/app.css :root and
// html[data-style=terminal]): monospace, 6 pt cards with a 1 pt edge, lowercase [bracket] keys in the accent.
// Follows System light/dark.

enum T {
    private static func hex(_ dark: UInt32, _ light: UInt32) -> Color {
        func ui(_ v: UInt32) -> UIColor {
            UIColor(red: CGFloat(v >> 16 & 0xff) / 255, green: CGFloat(v >> 8 & 0xff) / 255,
                    blue: CGFloat(v & 0xff) / 255, alpha: 1)
        }
        return Color(UIColor { $0.userInterfaceStyle == .light ? ui(light) : ui(dark) })
    }

    static let bg = hex(0x0f1a13, 0xf5efda)
    static let card = hex(0x1d1a12, 0xebe2c4)
    static let edge = hex(0x3b3a27, 0xcbbd90)
    static let line = hex(0x26301f, 0xe0d6b4)
    static let fg = hex(0xece4cc, 0x29251a)
    static let mute = hex(0xa3a68a, 0x5c573f)
    static let accent = hex(0xecdba6, 0x6b3410)
    static let green = hex(0x7fd06a, 0x2a6d1c)
    static let amber = hex(0xf29b4b, 0x7a5f00)
    static let red = hex(0xff7d6e, 0xb81d3c)

    static let radius: CGFloat = 6
    static let hit: CGFloat = 44

    static func font(_ size: CGFloat = 13, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    static func color(_ h: Health) -> Color {
        switch h {
        case .up: green
        case .down: red
        case .unknown: mute
        }
    }

    static func word(_ h: Health) -> String {
        switch h {
        case .up: "up"
        case .down: "down"
        case .unknown: "--"
        }
    }
}

/// A lowercase `[key]` in the accent colour, with a 44 pt hit area whatever its text width.
struct Key: View {
    let title: String
    var enabled = true
    var body: some View {
        Button {} label: {
            Text("[\(title.lowercased())]")
                .font(T.font())
                .foregroundStyle(enabled ? T.accent : T.mute)
                .frame(minWidth: T.hit, minHeight: T.hit)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

struct Dot: View {
    let color: Color
    var body: some View { Circle().fill(color).frame(width: 7, height: 7) }
}

struct TopBar: View {
    let title: String
    var key: String? = nil
    var body: some View {
        HStack(spacing: 0) {
            Text(title).font(T.font(15, .bold)).foregroundStyle(T.fg)
            Spacer()
            if let key { Key(title: key) }
        }
        .frame(minHeight: T.hit)
        .padding(.leading, 16).padding(.trailing, 10)
        .overlay(alignment: .bottom) { T.line.frame(height: 1) }
    }
}

/// A rounded card with Cooee's 1 pt edge and an optional coloured top rule.
struct Card<Content: View>: View {
    var rule: Color? = nil
    @ViewBuilder let content: Content
    var body: some View {
        VStack(alignment: .leading, spacing: 0) { content }
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(T.card)
            .overlay(alignment: .top) { if let rule { rule.frame(height: 3) } }
            .clipShape(RoundedRectangle(cornerRadius: T.radius))
            .overlay(RoundedRectangle(cornerRadius: T.radius).stroke(T.edge, lineWidth: 1))
    }
}

struct MonitorRow: View {
    let row: Row
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Dot(color: T.color(row.health))
                Text(row.name).foregroundStyle(T.fg).lineLimit(1).truncationMode(.tail)
                Spacer(minLength: 8)
                HStack(spacing: 0) {
                    Text(T.word(row.health)).foregroundStyle(T.color(row.health))
                    if !row.since.isEmpty { Text(" since \(row.since)").foregroundStyle(T.mute) }
                }
                .font(T.font(11)).fixedSize()
            }
            if !row.detail.isEmpty {
                Text(row.detail).font(T.font(11)).foregroundStyle(T.mute)
                    .lineLimit(1).truncationMode(.tail)
                    .padding(.leading, 15)
            }
        }
        .font(T.font())
        .padding(.horizontal, 12).padding(.vertical, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .leading) { if row.health == .down { T.red.frame(width: 3) } }
    }
}

struct GroupCard: View {
    let group: MonitorGroup
    var body: some View {
        let down = group.rows.filter { $0.health == .down }.count
        let unknown = group.rows.allSatisfy { $0.health == .unknown }
        let rule = unknown ? T.mute : down > 0 ? T.red : T.green
        Card(rule: rule) {
            HStack(spacing: 10) {
                Text("~ \(group.name.lowercased())").font(T.font(13, .bold)).foregroundStyle(T.fg)
                Group {
                    if unknown { Text("\(group.rows.count) unknown").foregroundStyle(T.mute) }
                    else if down > 0 { Text("\(down) down").foregroundStyle(T.red) }
                    else { Text("\(group.rows.count) up").foregroundStyle(T.mute) }
                }
                .font(T.font(11))
            }
            .padding(.horizontal, 12).padding(.top, 11).padding(.bottom, 5)
            ForEach(Array(group.rows.enumerated()), id: \.element.id) { i, row in
                if i > 0 { T.line.frame(height: 1).padding(.leading, 12) }
                MonitorRow(row: row)
            }
            Spacer().frame(height: 4)
        }
    }
}

/// A centred card for a list with nothing to show yet, in Cooee's "phrase · plain words" form.
struct StateCard: View {
    var glyph: String? = nil
    let phrase: String
    let plain: String
    var key: String? = nil
    var body: some View {
        Card {
            VStack(spacing: 10) {
                if let glyph { Text(glyph).font(T.font(28)).foregroundStyle(T.mute) }
                HStack(spacing: 0) {
                    Text(phrase).foregroundStyle(T.fg)
                    Text(" · \(plain)").font(T.font(11)).foregroundStyle(T.mute)
                }
                if let key { Key(title: key) }
            }
            .font(T.font())
            .frame(maxWidth: .infinity)
            .padding(.vertical, key == nil ? 28 : 18)
        }
    }
}

struct MonitorList: View {
    let state: ListState
    var body: some View {
        VStack(spacing: 0) {
            TopBar(title: "beacon", key: "cfg")
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if state.loading {
                        StateCard(glyph: "↻", phrase: "lighting up", plain: "checking 7 monitors")
                    } else if state.groups.isEmpty {
                        StateCard(phrase: "dark coast", plain: "no monitors", key: "change config")
                    } else {
                        summary
                        ForEach(state.groups) { GroupCard(group: $0) }
                    }
                }
                .padding(.horizontal, 12).padding(.vertical, 12)
            }
            .refreshable {}
        }
        .background(T.bg)
    }

    var summary: some View {
        let rows = state.groups.flatMap(\.rows)
        let down = rows.filter { $0.health == .down }.count
        let up = rows.filter { $0.health == .up }.count
        return VStack(alignment: .leading, spacing: 4) {
            if state.offline {
                HStack(spacing: 8) {
                    Dot(color: T.amber)
                    Text("no network").foregroundStyle(T.amber)
                    Text("· \(rows.count) unknown").font(T.font(11)).foregroundStyle(T.mute)
                }
            } else {
                HStack(spacing: 14) {
                    Text("\(down) down").foregroundStyle(down > 0 ? T.red : T.mute)
                    Text("\(up) up").foregroundStyle(T.green)
                }
                .font(T.font(13, .bold))
            }
            if let at = state.configFrom {
                HStack(spacing: 8) {
                    Dot(color: T.amber)
                    Text("config from \(at)").foregroundStyle(T.amber)
                    Text("· fetch failed").font(T.font(11)).foregroundStyle(T.mute)
                }
            }
        }
        .font(T.font())
        .padding(.horizontal, 4)
    }
}

struct Field: View {
    let label: String
    let value: String
    var placeholder = ""
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(T.font(11)).foregroundStyle(T.mute)
            Text(value.isEmpty ? placeholder : value)
                .foregroundStyle(value.isEmpty ? T.mute.opacity(0.7) : T.fg)
                .lineLimit(1).truncationMode(.middle)
                .frame(maxWidth: .infinity, minHeight: T.hit, alignment: .leading)
                .padding(.horizontal, 10)
                .background(T.card)
                .overlay(RoundedRectangle(cornerRadius: T.radius).stroke(T.edge, lineWidth: 1))
                .clipShape(RoundedRectangle(cornerRadius: T.radius))
        }
    }
}

/// The config-source controls: first run and settings use the same block.
struct SourceForm: View {
    var url = ""
    var headerName = ""
    var headerValue = ""
    var current: [(String, String)] = []
    var error: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if !current.isEmpty {
                Card(rule: T.accent) {
                    ForEach(Array(current.enumerated()), id: \.offset) { i, kv in
                        if i > 0 { T.line.frame(height: 1).padding(.leading, 12) }
                        HStack {
                            Text(kv.0).foregroundStyle(T.mute)
                            Spacer()
                            Text(kv.1).foregroundStyle(T.fg).lineLimit(1).truncationMode(.middle)
                        }
                        .padding(.horizontal, 12).padding(.vertical, 8)
                    }
                    Spacer().frame(height: 2)
                }
            }
            Field(label: "config url", value: url, placeholder: "https://…/monitors.json")
            Field(label: "header name (optional)", value: headerName, placeholder: "Authorization")
            Field(label: "header value (keychain)", value: headerValue.isEmpty ? "" : "••••••••")
            HStack(spacing: 4) {
                Key(title: "load", enabled: !url.isEmpty)
                Key(title: "import from files")
            }
            if let error {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Dot(color: T.red).alignmentGuide(.firstTextBaseline) { $0[.bottom] }
                    Text("refused").foregroundStyle(T.red)
                    Text(error).foregroundStyle(T.mute).lineLimit(2)
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(T.card)
                .overlay(alignment: .leading) { T.red.frame(width: 3) }
                .clipShape(RoundedRectangle(cornerRadius: T.radius))
            }
            Spacer()
        }
        .font(T.font())
        .padding(16)
    }
}

struct FirstRun: View {
    var error: String? = nil
    var body: some View {
        VStack(spacing: 0) {
            TopBar(title: "beacon")
            SourceForm(error: error)
        }
        .background(T.bg)
    }
}

struct Settings: View {
    var body: some View {
        MonitorList(state: Fixture.list)
            .sheet(isPresented: .constant(true)) {
                VStack(spacing: 0) {
                    TopBar(title: "config", key: "done")
                    SourceForm(
                        url: Fixture.configURL, headerName: "Authorization", headerValue: "x",
                        current: [("source", "url"), ("fetched", "08:40"), ("header", "Authorization ••••")])
                }
                .background(T.bg)
                .presentationBackground(T.bg)
            }
    }
}

// MARK: Widgets

struct DownNames: View {
    let names: [String]
    let max: Int
    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            ForEach(names.prefix(max), id: \.self) { name in
                HStack(spacing: 6) {
                    Dot(color: T.red)
                    Text(name).foregroundStyle(T.fg).lineLimit(1).truncationMode(.tail)
                }
            }
            if names.count > max { Text("+\(names.count - max) more").foregroundStyle(T.mute) }
        }
    }
}

struct Counts: View {
    let state: WidgetState
    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            if state.downNames.isEmpty {
                Text("all up").foregroundStyle(T.green).font(T.font(15, .bold))
            } else {
                Text("\(state.downNames.count) down").foregroundStyle(T.red).font(T.font(15, .bold))
            }
            Text("\(state.up) up").foregroundStyle(state.downNames.isEmpty ? T.mute : T.green)
        }
    }
}

struct SmallWidget: View {
    let state: WidgetState
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Counts(state: state)
            Spacer(minLength: 6)
            DownNames(names: state.downNames, max: 3)
            Spacer(minLength: 6)
            Text("checked \(state.checkedAt)").foregroundStyle(T.mute).font(T.font(10))
        }
        .font(T.font(11))
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct MediumWidget: View {
    let state: WidgetState
    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            VStack(alignment: .leading, spacing: 0) {
                Text("beacon").font(T.font(11, .bold)).foregroundStyle(T.mute).padding(.bottom, 6)
                Counts(state: state)
                Spacer()
                Text("checked \(state.checkedAt)").foregroundStyle(T.mute).font(T.font(10))
            }
            .frame(width: 96, alignment: .leading)
            DownNames(names: state.downNames, max: 5)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .overlay(alignment: .leading) { T.line.frame(width: 1).offset(x: -8) }
        }
        .font(T.font(11))
    }
}

#Preview("list") { MonitorList(state: Fixture.list) }
#Preview("list offline") { MonitorList(state: Fixture.offline) }
#Preview("list empty") { MonitorList(state: Fixture.empty) }
#Preview("list loading") { MonitorList(state: Fixture.loading) }
#Preview("first run") { FirstRun() }
#Preview("settings") { Settings() }
#Preview("widgets") { WidgetBoard() }
