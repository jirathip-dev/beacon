import SwiftUI

// Look A: plain iOS. System list, SF Symbols, light/dark, one accent (indigo).

enum A {
    static let accent = Color.indigo

    static func color(_ h: Health) -> Color {
        switch h {
        case .up: .green
        case .down: .red
        case .unknown: .gray
        }
    }
}

struct ARow: View {
    let row: Row
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Circle().fill(A.color(row.health)).frame(width: 10, height: 10)
            VStack(alignment: .leading, spacing: 2) {
                Text(row.name)
                if !row.detail.isEmpty {
                    Text(row.detail).font(.footnote).foregroundStyle(.secondary)
                }
            }
            Spacer()
            if !row.since.isEmpty {
                Text("since \(row.since)").font(.footnote).foregroundStyle(.secondary).monospacedDigit()
            }
        }
    }
}

struct AList: View {
    let state: ListState
    var body: some View {
        NavigationStack {
            List {
                if let at = state.configFrom {
                    Label("config from \(at)", systemImage: "exclamationmark.icloud")
                        .font(.footnote).foregroundStyle(.secondary).listRowBackground(Color.clear)
                }
                if state.offline {
                    Label("No network", systemImage: "wifi.slash")
                        .font(.footnote).foregroundStyle(.secondary).listRowBackground(Color.clear)
                }
                ForEach(state.groups) { group in
                    Section(group.name) {
                        ForEach(group.rows) { ARow(row: $0) }
                    }
                }
            }
            .listStyle(.insetGrouped)
            .refreshable {}
            .navigationTitle("Beacon")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Settings", systemImage: "gearshape") {}
                }
            }
        }
        .tint(A.accent)
    }
}

/// The config-source controls: first run and Settings use the same form.
struct ASourceForm: View {
    var url = ""
    var headerName = ""
    var headerValue = ""
    var current: String? = nil
    var error: String? = nil

    var body: some View {
        Form {
            if let current {
                Section("Current") {
                    LabeledContent("Source", value: current)
                    LabeledContent("Header", value: headerName.isEmpty ? "None" : "\(headerName) ••••")
                }
            }
            Section {
                TextField("https://…/monitors.json", text: .constant(url))
                    .keyboardType(.URL).textInputAutocapitalization(.never)
            } header: {
                Text("Config URL")
            }
            Section {
                TextField("Name, e.g. Authorization", text: .constant(headerName))
                SecureField("Value", text: .constant(headerValue))
            } header: {
                Text("Header")
            } footer: {
                Text("Optional. Kept in the Keychain.")
            }
            Section {
                Button("Load") {}.disabled(url.isEmpty)
                Button("Import from Files", systemImage: "folder") {}
            } footer: {
                if let error {
                    Label(error, systemImage: "xmark.octagon.fill").foregroundStyle(.red)
                }
            }
        }
        .tint(A.accent)
    }
}

struct AFirstRun: View {
    var error: String? = nil
    var body: some View {
        NavigationStack {
            ASourceForm(error: error).navigationTitle("Beacon")
        }
    }
}

struct ASettings: View {
    var body: some View {
        AList(state: Fixture.list)
            .sheet(isPresented: .constant(true)) {
                NavigationStack {
                    ASourceForm(
                        url: Fixture.configURL, headerName: "Authorization", headerValue: "secret-token",
                        current: "URL, fetched 08:40")
                        .navigationTitle("Settings").navigationBarTitleDisplayMode(.inline)
                        .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") {} } }
                }
                .tint(A.accent)
            }
    }
}

// MARK: Widgets (the widget reads only StatusSnapshot: counts, down names, last check)

struct ASmallWidget: View {
    let state: WidgetState
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Image(systemName: state.downNames.isEmpty ? "checkmark.circle.fill" : "xmark.circle.fill")
                    .foregroundStyle(state.downNames.isEmpty ? .green : .red)
                Text(state.downNames.isEmpty ? "All up" : "\(state.downNames.count) down")
                    .font(.headline)
            }
            Text("\(state.up) up").font(.footnote).foregroundStyle(.secondary)
            Spacer(minLength: 2)
            ForEach(state.downNames.prefix(3), id: \.self) { name in
                Text(name).font(.caption).lineLimit(1)
            }
            if state.downNames.count > 3 {
                Text("+\(state.downNames.count - 3) more").font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 2)
            Text(state.checkedAt).font(.caption2).foregroundStyle(.secondary).monospacedDigit()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

struct AMediumWidget: View {
    let state: WidgetState
    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text("\(state.downNames.count)").font(.system(size: 40, weight: .semibold, design: .rounded))
                    .foregroundStyle(state.downNames.isEmpty ? .green : .red)
                Text("down").font(.footnote).foregroundStyle(.secondary)
                Text("\(state.up) up").font(.footnote).foregroundStyle(.secondary)
                Spacer()
                Text(state.checkedAt).font(.caption2).foregroundStyle(.secondary).monospacedDigit()
            }
            .frame(width: 70, alignment: .leading)
            VStack(alignment: .leading, spacing: 6) {
                ForEach(state.downNames.prefix(5), id: \.self) { name in
                    HStack(spacing: 6) {
                        Circle().fill(.red).frame(width: 8, height: 8)
                        Text(name).font(.subheadline).lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

#Preview("A list") { AList(state: Fixture.list) }
#Preview("A list offline") { AList(state: Fixture.offline) }
#Preview("A first run") { AFirstRun() }
#Preview("A settings") { ASettings() }
#Preview("A widgets") { WidgetBoard(look: .a) }
