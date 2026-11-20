import BeaconKit
import SwiftUI

/// What a history screen shows: one check on its own host, or one http/tcp monitor on the `prober`.
struct HistoryTarget: Equatable {
    let title: String
    let sub: String
    let host: String?
    var check: String?
}

/// Uptime, a 30-day strip, the incident log and, for an http/tcp monitor, its response times (ADR H4).
struct HistoryScreen: View {
    let model: Model
    let target: HistoryTarget
    let monitor: Monitor?
    let close: () -> Void
    @State private var shown: (body: HistoryBody, etag: String?, at: Int)?
    @State private var times: (body: TimesBody, etag: String?)?
    @State private var note: HostDetail.Note?
    @State private var notAfter: Date?
    @State private var range = "24h"
    @Environment(\.scenePhase) private var phase

    var host: Host? { target.host.flatMap { model.config?.hosts?[$0] } }

    var body: some View {
        VStack(spacing: 0) {
            Header(title: target.title, sub: target.sub, back: target.check == nil ? "monitors" : target.host ?? "", close: close)
            ScrollView {
                VStack(alignment: .leading, spacing: 12) { content }
                    .font(T.font()).padding(.horizontal, 16).padding(.vertical, 12)
            }
        }
        .background(T.bg)
        .task(id: phase) {
            while phase == .active, !Task.isCancelled {
                await fetch()
                try? await Task.sleep(for: .seconds(60))
            }
        }
    }

    @ViewBuilder var content: some View {
        if let monitor { about(monitor) }
        let track = shown.flatMap { s in target.check.map { s.body.checks[$0] } ?? monitor.flatMap { s.body.track(of: $0)?.track } }
        if case .unreachable(let why) = note, let at = shown?.at {
            HStack(spacing: 6) { Dot(color: T.amber); Text("history from \(hhmm(at)) · \(why)") }.foregroundStyle(T.amber)
        }
        if case .notServed = note {
            WordCard(art: true, phrase: "dark lamp · no history")
        } else if let b = shown?.body, let track {
            Group {
                uptime(track, b)
                if monitor != nil, let t = times?.body { chart(t) }
                incidents(track)
            }
            .opacity(note == nil ? 1 : 0.45)
        } else if host == nil || shown != nil {
            WordCard(phrase: "no history")
        } else if case .unreachable(let why) = note {
            WordCard(phrase: "● history unreachable · \(why)", color: T.amber)
        } else {
            WordCard(phrase: "↻ reading the log")
        }
    }

    func fetch() async {
        if case .http(let m) = monitor, notAfter == nil {
            notAfter = (try? await LiveTransport().get(URLRequest(url: m.url), timeout: m.timeout_s ?? 10))?.2
        }
        guard let host else { return }
        let (fetcher, timeout, now) = (HostMetrics(), host.timeout_s ?? 5, Int(Date.now.timeIntervalSince1970))
        switch await fetcher.fetch(host.historyURL, etag: shown?.etag, timeout: timeout, decode: HistoryBody.decode) {
        case .fresh(let body, let etag): (shown, note) = ((body, etag, now), nil)
        case .unchanged: (shown, note) = (shown.map { ($0.body, $0.etag, now) }, nil)
        case .notServed: (shown, note) = (nil, .notServed)
        case .unreachable(let why): note = .unreachable(why)
        }
        guard let id = monitor.flatMap({ shown?.body.track(of: $0)?.id }) else { return }
        let url = host.historyURL(id: id)
        if case .fresh(let body, let etag) = await fetcher.fetch(url, etag: times?.etag, timeout: timeout, decode: TimesBody.decode) {
            times = (body, etag)
        }
    }

    func about(_ m: Monitor) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            switch m {
            case .http(let h): Text(h.url.absoluteString)
            case .tcp(let t): Text("\(t.host):\(t.port)")
            case .heartbeat: EmptyView()
            }
            if case .http(let h) = m, let notAfter {
                let days = Int(notAfter.timeIntervalSinceNow / 86_400), bad = days < h.cert_warn_days ?? 14
                HStack(spacing: 6) {
                    Dot(color: bad ? T.red : T.green)
                    Text("cert \(bad ? "ends" : "good to") \(md(notAfter)) · \(days)d left").foregroundStyle(bad ? T.red : T.fg)
                }
            }
        }
        .font(T.font(12)).foregroundStyle(T.mute).padding(.horizontal, 4)
    }

    func uptime(_ track: Track, _ b: HistoryBody) -> some View {
        let (now, day) = (Int(Date.now.timeIntervalSince1970), 86_400)
        let start = Int(Calendar.current.startOfDay(for: .now).timeIntervalSince1970)
        let month = track.tally(last: 30 * day, gaps: b.gaps, now: now)
        let gaps = b.gaps.filter { ($0.last ?? 0) > max(now - 30 * day, track.since) }
            .map { " · \(dur(($0.last ?? 0) - ($0.first ?? 0))) not watched on \(md(at($0.first ?? 0)))" }
        return Card {
            CardHead(title: "~ uptime", note: "as seen from \(b.host)")
            HStack(alignment: .top) {
                ForEach([(day, "last 24h"), (30 * day, "last 30d")], id: \.0) { w, name in
                    let span = track.tally(last: w, gaps: b.gaps, now: now).span
                    VStack(alignment: .leading, spacing: 2) {
                        Text(span < w - 60 ? "\(dur(span)), all it has" : name).font(T.font(11)).foregroundStyle(T.mute)
                        Text(pct(track.uptime(last: w, gaps: b.gaps, now: now))).font(T.font(17, .bold)).foregroundStyle(T.fg)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 10)
            HStack(spacing: 3) {
                ForEach(0..<30, id: \.self) { k in
                    let a = start - (29 - k) * day, e = min(a + day, now)
                    let t = track.tally(last: e - a, gaps: b.gaps, now: e)
                    RoundedRectangle(cornerRadius: 2)
                        .fill(a + day <= track.since ? .clear : t.watched <= 0 ? T.edge : t.down > 0 ? T.red : T.green)
                        .stroke(a + day <= track.since ? T.edge : .clear)
                        .frame(height: 22)
                }
            }
            .padding(.horizontal, 12)
            HStack { Text(md(at(start - 29 * day))); Spacer(); Text("today") }.padding(.horizontal, 12).padding(.top, 4)
            Text("watched \(dur(month.watched))" + gaps.joined()).padding(12)
        }
        .font(T.font(11)).foregroundStyle(T.mute)
    }

    func chart(_ t: TimesBody) -> some View {
        let month = range == "30d"
        let w = month ? Window(t.hourly.t, t.hourly.avg_ms, seconds: 30 * 86_400, bucket: 4)
            : Window(t.recent.t, t.recent.ms, seconds: range == "1h" ? 3600 : 86_400, bucket: range == "1h" ? 1 : 7)
        let peak = month ? Window(t.hourly.t, t.hourly.max_ms, seconds: 30 * 86_400, bucket: 4) : nil
        let hi = Swift.max(1, (peak ?? w).points.compactMap(\.y).max() ?? 1) * 1.05
        let top = [40.0, 80, 100, 200, 400, 800, 1000, 2000, 4000, 8000].first { $0 >= hi } ?? (hi / 4000).rounded(.up) * 4000
        let misses = month ? [] : zip(w.t, w.values).filter { $0.1 == nil }.map(\.0)
        let ms = { (v: Double?) in v.map { "\(Int($0.rounded()))ms" } ?? "--" }
        return Card {
            HStack(spacing: 0) {
                Text("~ response time").foregroundStyle(T.fg)
                Spacer()
                ForEach(["1h", "24h", "30d"], id: \.self) { r in RangeKey(title: r, on: r == range) { range = r } }
            }
            .padding(.leading, 12)
            Plot(w: w, top: top, step: month ? 7 * 86_400 : range == "1h" ? 900 : 21_600, unit: "ms", read: ms) { ctx, rect, x in
                let y = { (v: Double) in rect.maxY - CGFloat(v / top) * rect.height }
                let pairs = zip(w.points, peak?.points ?? [])
                for (a, b) in zip(pairs, pairs.dropFirst()) {
                    guard let a0 = a.0.y, let a1 = a.1.y, let b0 = b.0.y, let b1 = b.1.y else { continue }
                    let quad = [(a.0.t, a0), (b.0.t, b0), (b.0.t, b1), (a.0.t, a1)].map { CGPoint(x: x($0.0), y: y($0.1)) }
                    ctx.fill(Path { $0.addLines(quad) }, with: .color(T.accent.opacity(0.18)))
                }
                for m in misses {
                    ctx.stroke(Path { $0.addLines([CGPoint(x: x(m), y: rect.maxY - 7), CGPoint(x: x(m), y: rect.maxY)]) },
                               with: .color(T.red), lineWidth: 2)
                }
            }
            .id(range)
            if month {
                HStack(spacing: 14) {
                    HStack(spacing: 4) { T.accent.frame(width: 18, height: 1.5); Text("avg") }
                    HStack(spacing: 4) { T.accent.opacity(0.18).frame(width: 18, height: 8); Text("max") }
                }
                .font(T.font(11)).foregroundStyle(T.mute).frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12).padding(.bottom, 6)
            }
            T.line.frame(height: 1)
            let num = { (v: Double?) in v.map { "\(Int($0.rounded()))" } ?? "--" }
            Stats(items: [("now", num(t.recent.ms.last { $0 != nil } ?? nil)), ("min", num(w.min)), ("avg", num(w.avg)),
                          ("max", num((peak ?? w).max))], unit: "ms")
            if !misses.isEmpty {
                T.line.frame(height: 1)
                Text("\(Text("▮").foregroundStyle(T.red)) \(misses.count) no response").font(T.font(11))
                    .foregroundStyle(T.mute).padding(12)
            }
        }
    }

    func incidents(_ track: Track) -> some View {
        let now = Int(Date.now.timeIntervalSince1970)
        let all = Array(track.down.filter { ($0.up_at ?? now) > now - 30 * 86_400 }.reversed())
        return Card {
            CardHead(title: "~ incidents", note: all.isEmpty ? "none in 30d" : "\(min(5, all.count)) of \(all.count) · 30d")
            ForEach(all.prefix(5), id: \.self) { i in
                let open = i.up_at == nil
                if i != all.first { T.line.frame(height: 1) }
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Dot(color: T.red)
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            if let up = i.up_at {
                                let same = Calendar.current.isDate(at(up), inSameDayAs: at(i.down_at))
                                Text("\(stamp(i.down_at)) → \(same ? hhmm(up) : stamp(up))")
                                Spacer()
                                Text(dur(up - i.down_at))
                            } else {
                                Text("down since \(stamp(i.down_at))")
                                Spacer()
                                Text("\(dur(now - i.down_at)) so far")
                            }
                        }
                        .foregroundStyle(open ? T.red : T.fg)
                        Text(i.message).font(T.font(11)).foregroundStyle(T.mute)
                    }
                }
                .padding(12)
                .overlay(alignment: .leading) { if open { T.red.frame(width: 3) } }
            }
        }
    }
}

struct CardHead: View {
    let title: String
    let note: String
    var body: some View {
        HStack { Text(title).foregroundStyle(T.fg); Spacer(); Text(note).foregroundStyle(T.mute) }
            .font(T.font()).padding(.horizontal, 12).padding(.vertical, 8)
            .overlay(alignment: .bottom) { T.line.frame(height: 1) }
    }
}

struct Header: View {
    let title: String
    var sub: String?
    let back: String
    let close: () -> Void
    var body: some View {
        ZStack {
            VStack(spacing: 0) {
                Text(title).font(T.font(15, .bold)).foregroundStyle(T.fg)
                if let sub { Text(sub).font(T.font(11)).foregroundStyle(T.mute) }
            }
            HStack { Key(title: "‹ \(back)", action: close); Spacer() }
        }
        .frame(minHeight: T.hit)
        .padding(.horizontal, 12)
        .overlay(alignment: .bottom) { T.line.frame(height: 1) }
    }
}

struct RangeKey: View {
    let title: String
    let on: Bool
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Text("[\(title)]").fontWeight(on ? .bold : .regular).underline(on).foregroundStyle(on ? T.fg : T.accent)
                .padding(.horizontal, 8).frame(minWidth: T.hit, minHeight: T.hit)
        }
        .buttonStyle(.plain)
    }
}

func at(_ t: Int) -> Date { Date(timeIntervalSince1970: TimeInterval(t)) }
func md(_ d: Date) -> String { d.formatted(.dateTime.month(.abbreviated).day()) }
func stamp(_ t: Int) -> String { Calendar.current.isDateInToday(at(t)) ? hhmm(t) : "\(md(at(t))) \(hhmm(t))" }
func pct(_ u: Double?) -> String { u.map { $0 >= 1 ? "100%" : String(format: "%.2f%%", ($0 * 10_000).rounded(.down) / 100) } ?? "--" }

func dur(_ s: Int) -> String {
    let m = (s + 30) / 60, h = m / 60, d = h / 24
    return d > 0 ? "\(d)d \(h % 24)h" : h > 0 ? "\(h)h \(m % 60)m" : "\(max(1, m))m"
}
