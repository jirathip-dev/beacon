import SwiftUI

// Look B: terminal. Dark-first, monospace, dense rows, status as coloured text.

enum B {
    static let bg = Color(red: 0.04, green: 0.05, blue: 0.04)
    static let fg = Color(red: 0.80, green: 0.84, blue: 0.80)
    static let dim = Color(red: 0.45, green: 0.48, blue: 0.45)
    static let line = Color(red: 0.18, green: 0.20, blue: 0.18)
    static let green = Color(red: 0.25, green: 0.82, blue: 0.48)
    static let red = Color(red: 1.0, green: 0.36, blue: 0.34)

    static func font(_ size: CGFloat = 13, _ weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }

    static func token(_ h: Health) -> (String, Color) {
        switch h {
        case .up: ("up  ", green)
        case .down: ("DOWN", red)
        case .unknown: ("--  ", dim)
        }
    }
}

struct BRow: View {
    let row: Row
    var body: some View {
        let (word, color) = B.token(row.health)
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 8) {
                Text(word).foregroundStyle(color)
                Text(row.name).foregroundStyle(B.fg).lineLimit(1)
                Spacer(minLength: 4)
                if !row.since.isEmpty { Text("since \(row.since)").foregroundStyle(B.dim) }
            }
            if !row.detail.isEmpty {
                Text("     \(row.detail)").foregroundStyle(row.health == .down ? B.red.opacity(0.8) : B.dim)
                    .lineLimit(2)
            }
        }
        .font(B.font())
        .padding(.vertical, 3)
    }
}

struct BTopBar: View {
    let title: String
    let action: String
    var body: some View {
        HStack {
            Text(title).font(B.font(15, .bold)).foregroundStyle(B.fg)
            Spacer()
            Text(action).font(B.font(13)).foregroundStyle(B.green)
        }
        .padding(.horizontal, 16).padding(.vertical, 10)
        .overlay(alignment: .bottom) { B.line.frame(height: 1) }
    }
}

struct BList: View {
    let state: ListState
    var body: some View {
        VStack(spacing: 0) {
            BTopBar(title: "beacon", action: "[cfg]")
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    summary.padding(.bottom, 6)
                    ForEach(state.groups) { group in
                        Text("# \(group.name.lowercased())").font(B.font(12)).foregroundStyle(B.dim)
                            .padding(.top, 10).padding(.bottom, 2)
                        ForEach(group.rows) { BRow(row: $0) }
                    }
                }
                .padding(.horizontal, 16).padding(.top, 8)
            }
            .refreshable {}
        }
        .background(B.bg)
        .preferredColorScheme(.dark)
    }

    var summary: some View {
        let rows = state.groups.flatMap(\.rows)
        let down = rows.filter { $0.health == .down }.count
        let up = rows.filter { $0.health == .up }.count
        return VStack(alignment: .leading, spacing: 2) {
            if state.offline {
                Text("no network: \(rows.count) unknown").foregroundStyle(B.dim)
            } else {
                HStack(spacing: 12) {
                    Text("\(down) down").foregroundStyle(down > 0 ? B.red : B.dim)
                    Text("\(up) up").foregroundStyle(B.green)
                }
            }
            if let at = state.configFrom {
                Text("config from \(at)").foregroundStyle(B.dim)
            }
        }
        .font(B.font())
    }
}

struct BField: View {
    let label: String
    let value: String
    var placeholder = ""
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).foregroundStyle(B.dim)
            Text(value.isEmpty ? placeholder : value)
                .foregroundStyle(value.isEmpty ? B.dim.opacity(0.6) : B.fg).lineLimit(1).truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 8).padding(.vertical, 8)
                .overlay(Rectangle().stroke(B.line, lineWidth: 1))
        }
    }
}

struct BButton: View {
    let title: String
    var enabled = true
    var body: some View {
        Text("[ \(title) ]").foregroundStyle(enabled ? B.green : B.dim)
    }
}

/// The config-source controls: first run and settings use the same block.
struct BSourceForm: View {
    var url = ""
    var headerName = ""
    var headerValue = ""
    var current: [String] = []
    var error: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if !current.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(current, id: \.self) { Text($0).foregroundStyle(B.fg) }
                }
            }
            BField(label: "config url", value: url, placeholder: "https://…/monitors.json")
            BField(label: "header name (optional)", value: headerName, placeholder: "Authorization")
            BField(label: "header value (keychain)", value: headerValue.isEmpty ? "" : "••••••••", placeholder: "")
            HStack(spacing: 16) {
                BButton(title: "load", enabled: !url.isEmpty)
                BButton(title: "import from files")
            }
            if let error {
                Text("error: \(error)").foregroundStyle(B.red)
            }
            Spacer()
        }
        .font(B.font())
        .padding(16)
    }
}

struct BFirstRun: View {
    var error: String? = nil
    var body: some View {
        VStack(spacing: 0) {
            BTopBar(title: "beacon", action: "")
            BSourceForm(error: error)
        }
        .background(B.bg)
        .preferredColorScheme(.dark)
    }
}

struct BSettings: View {
    var body: some View {
        BList(state: Fixture.list)
            .sheet(isPresented: .constant(true)) {
                VStack(spacing: 0) {
                    BTopBar(title: "settings", action: "[done]")
                    BSourceForm(
                        url: Fixture.configURL, headerName: "Authorization", headerValue: "x",
                        current: ["source  url", "fetched 08:40", "header  Authorization ••••"])
                }
                .background(B.bg)
                .presentationBackground(B.bg)
                .preferredColorScheme(.dark)
            }
    }
}

// MARK: Widgets

struct BSmallWidget: View {
    let state: WidgetState
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("\(state.downNames.count) down").foregroundStyle(state.downNames.isEmpty ? B.dim : B.red)
                .font(B.font(15, .bold))
            Text("\(state.up) up").foregroundStyle(B.green)
            Spacer(minLength: 4)
            ForEach(state.downNames.prefix(3), id: \.self) { Text("x \($0)").foregroundStyle(B.fg).lineLimit(1) }
            if state.downNames.count > 3 { Text("+\(state.downNames.count - 3)").foregroundStyle(B.dim) }
            Spacer(minLength: 4)
            Text("@ \(state.checkedAt)").foregroundStyle(B.dim)
        }
        .font(B.font(12))
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct BMediumWidget: View {
    let state: WidgetState
    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            VStack(alignment: .leading, spacing: 2) {
                Text("\(state.downNames.count) down").foregroundStyle(state.downNames.isEmpty ? B.dim : B.red)
                    .font(B.font(15, .bold))
                Text("\(state.up) up").foregroundStyle(B.green)
                Spacer()
                Text("@ \(state.checkedAt)").foregroundStyle(B.dim)
            }
            .frame(width: 96, alignment: .leading)
            VStack(alignment: .leading, spacing: 3) {
                ForEach(state.downNames.prefix(5), id: \.self) { name in
                    HStack(spacing: 6) {
                        Text("DOWN").foregroundStyle(B.red)
                        Text(name).foregroundStyle(B.fg).lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(B.font(12))
    }
}

#Preview("B list") { BList(state: Fixture.list) }
#Preview("B list offline") { BList(state: Fixture.offline) }
#Preview("B first run") { BFirstRun() }
#Preview("B settings") { BSettings() }
#Preview("B widgets") { WidgetBoard(look: .b) }
