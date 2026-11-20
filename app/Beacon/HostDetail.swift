import BeaconKit
import SwiftUI

/// A host's load, cpu, mem and disks from its `/metrics`, fetched only while this screen is open (ADR D10).
struct HostDetail: View {
    let model: Model
    let name: String
    let host: Host
    let close: () -> Void
    @State private var shown: (body: MetricsBody, etag: String?)?
    @State private var note: Note?
    @State private var chart: Series?
    @Environment(\.scenePhase) private var phase

    enum Note { case notServed, unreachable(String) }

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                Text(name).font(T.font(15, .bold)).foregroundStyle(T.fg)
                HStack { Key(title: "‹ monitors", action: close); Spacer() }
            }
            .frame(minHeight: T.hit)
            .padding(.horizontal, 12)
            .overlay(alignment: .bottom) { T.line.frame(height: 1) }
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    metrics
                    let rows = model.snapshot?.rows.filter { model.host(of: $0) == name } ?? []
                    if !rows.isEmpty { GroupCard(name: "checks on \(name)", rows: rows, ruled: false) }
                }
                .font(T.font())
                .padding(.horizontal, 16).padding(.vertical, 12)
            }
        }
        .background(T.bg)
        .task(id: phase) {
            while phase == .active, !Task.isCancelled {
                await fetch()
                try? await Task.sleep(for: .seconds(60))
            }
        }
        .sheet(item: $chart) { s in
            if let b = shown?.body {
                ChartSheet(series: b.series(order: host.disks).first { $0.id == s.id } ?? s, t: b.t, host: name) {
                    chart = nil
                }
            }
        }
    }

    func fetch() async {
        switch await HostMetrics().fetch(host.metricsURL, etag: shown?.etag, timeout: host.timeout_s ?? 5) {
        case .fresh(let body, let etag): (shown, note) = ((body, etag), nil)
        case .unchanged: note = nil
        case .notServed: (shown, note) = (nil, .notServed)
        case .unreachable(let why): note = .unreachable(why)
        }
    }

    @ViewBuilder var metrics: some View {
        if let b = shown?.body, let last = b.t.last {
            let dim = note != nil
            if case .unreachable(let why) = note {
                HStack(spacing: 6) { Dot(color: T.amber); Text("metrics from \(hhmm(last)) · \(why)") }
                    .foregroundStyle(T.amber).padding(.horizontal, 4)
            }
            Card {
                HStack { Text("~ host").foregroundStyle(T.fg); Spacer(); Text("last 1h").foregroundStyle(T.mute) }
                    .padding(.horizontal, 12).padding(.vertical, 8)
                ForEach(b.series(order: host.disks)) { s in
                    T.line.frame(height: 1)
                    MetricRow(series: s, sub: sub(s, b), t: b.t, dim: dim)
                        .contentShape(Rectangle())
                        .onTapGesture { chart = s }
                }
            }
            Text("updated \(hhmm(last)) · a sample every \(b.every_s)s · tap a row for its chart")
                .font(T.font(11)).foregroundStyle(T.mute).padding(.horizontal, 4)
        } else if case .notServed = note {
            WordCard(art: true, phrase: "dark lamp · no metrics",
                     plain: "\(name)'s beacon serve is older than metrics. update beacon there to see its history.")
        } else if case .unreachable(let why) = note {
            WordCard(phrase: "● metrics unreachable · \(why)", plain: "trying \(name) again every minute", color: T.amber)
        } else if shown != nil {
            WordCard(phrase: "just lit", plain: "no samples yet · the first comes within a minute")
        } else {
            WordCard(phrase: "↻ reading the lamp", plain: "fetching \(name) metrics")
        }
    }

    /// The plain line under a metric's value.
    func sub(_ s: Series, _ b: MetricsBody) -> String {
        let last = { (xs: [Int?]) in xs.last { $0 != nil } ?? nil }
        let gb = { (v: Int?) -> String in
            guard let v else { return "--" }
            let g = Double(v) / 1_073_741_824
            return String(format: g >= 100 || g == g.rounded() ? "%.0f" : "%.1f", g)
        }
        let two = { (xs: [Double?]) in (xs.last { $0 != nil } ?? nil).map { String(format: "%.2f", $0) } ?? "--" }
        switch s.id {
        case "load": return "5m \(two(b.load5)) · 15m \(two(b.load15))"
        case "cpu": return "all cores"
        case "mem": return "\(gb(last(b.mem.used_bytes))) of \(gb(b.mem.total_bytes)) GB · swap \(gb(last(b.swap.used_bytes)))"
        default:
            let d = b.disks[String(s.id.dropFirst(5))]
            let (u, f) = (last(d?.used_bytes ?? []), last(d?.free_bytes ?? []))
            return "\(gb(f)) of \(gb(u.flatMap { u in f.map { u + $0 } })) GB free"
        }
    }
}

func hhmm(_ t: Int) -> String {
    Date(timeIntervalSince1970: TimeInterval(t)).formatted(.dateTime.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
}

/// A value as the design writes it: load with two decimals, cpu with one, the rest whole.
func value(_ s: Series, _ v: Double?) -> String {
    v.map { String(format: s.id == "load" ? "%.2f" : s.id == "cpu" ? "%.1f" : "%.0f", $0) } ?? "--"
}

struct MetricRow: View {
    let series: Series
    let sub: String
    let t: [Int]
    let dim: Bool
    var body: some View {
        let hour = Window(t, series.values, range: .hour)
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(series.id).font(T.font(11)).foregroundStyle(T.mute)
                Text("\(Text(value(series, series.now)).font(T.font(17, .bold)).foregroundStyle(dim ? T.mute : T.fg))\(Text(series.percent ? "%" : "").font(T.font(11)).foregroundStyle(T.mute))")
                Text(sub).font(T.font(11)).foregroundStyle(T.mute)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Canvas { ctx, size in
                ctx.stroke(Path { $0.addLines([CGPoint(x: 0, y: size.height), CGPoint(x: size.width, y: size.height)]) },
                           with: .color(T.line))
                trace(ctx, hour, top: series.top(hour.values), in: CGRect(origin: .zero, size: size), dot: true)
            }
            .frame(width: 132, height: 36)
            .opacity(dim ? 0.45 : 1)
            Text("›").foregroundStyle(T.mute).frame(width: 14, alignment: .leading)
        }
        .padding(.horizontal, 12).padding(.vertical, 10)
        .frame(minHeight: 64)
    }
}

/// Draws `w`'s points in `rect` as an accent line over a faint fill, broken at every missing point.
func trace(_ ctx: GraphicsContext, _ w: Window, top: Double, in rect: CGRect, dot: Bool) {
    let (t0, t1) = (w.t.first ?? 0, w.t.last ?? 0)
    let x = { (t: Int) in rect.minX + (t1 == t0 ? rect.width : CGFloat(t - t0) / CGFloat(t1 - t0) * rect.width) }
    var line = Path(), area = Path(), run: [CGPoint] = [], newest: CGPoint?
    func flush() {
        guard let first = run.first, let last = run.last else { return }
        line.addLines(run)
        area.addLines([CGPoint(x: first.x, y: rect.maxY)] + run + [CGPoint(x: last.x, y: rect.maxY)])
        area.closeSubpath()
        run = []
    }
    for p in w.points {
        guard let y = p.y else { flush(); continue }
        let point = CGPoint(x: x(p.t), y: rect.maxY - CGFloat(y / top) * rect.height)
        newest = point
        run.append(point)
    }
    flush()
    ctx.fill(area, with: .color(T.accent.opacity(0.12)))
    ctx.stroke(line, with: .color(T.accent), style: StrokeStyle(lineWidth: 1.5, lineCap: .round, lineJoin: .round))
    if dot, let newest {
        let r = CGRect(x: newest.x - 3, y: newest.y - 3, width: 6, height: 6)
        ctx.fill(Path(ellipseIn: r), with: .color(T.accent))
    }
}

/// A centred card in Cooee's "phrase · plain words" form, with the lighthouse art when the lamp is dark.
struct WordCard: View {
    var art = false
    let phrase: String
    let plain: String
    var color = T.fg
    var body: some View {
        Card {
            VStack(spacing: 6) {
                if art {
                    Image("Lamp").resizable().frame(width: 72, height: 72)
                        .clipShape(RoundedRectangle(cornerRadius: 16)).padding(.bottom, 6)
                }
                Text(phrase).foregroundStyle(color)
                Text(plain).foregroundStyle(T.mute)
            }
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 28).padding(.horizontal, 16)
        }
    }
}

/// One metric over 1h, 6h or 24h; hold and drag to read a point.
struct ChartSheet: View {
    let series: Series
    let t: [Int]
    let host: String
    let done: () -> Void
    @State private var range = ChartRange.hour
    @State private var scrub: Int?

    var body: some View {
        let w = Window(t, series.values, range: range)
        let unit = series.percent ? "%" : ""
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 0) {
                Text("\(Text(series.id).bold()) \(Text("· \(host)").foregroundStyle(T.mute))").font(T.font(15))
                Spacer()
                Key(title: "done", action: done)
            }
            .foregroundStyle(T.fg)
            .frame(minHeight: T.hit).padding(.horizontal, 16).padding(.top, 8)
            .overlay(alignment: .bottom) { T.line.frame(height: 1) }
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 0) {
                    ForEach(ChartRange.allCases, id: \.self) { r in
                        Button { (range, scrub) = (r, nil) } label: {
                            Text("[\(r.rawValue)]").fontWeight(r == range ? .bold : .regular)
                                .underline(r == range).foregroundStyle(r == range ? T.fg : T.accent)
                                .padding(.horizontal, 8).frame(minWidth: T.hit, minHeight: T.hit)
                        }
                        .buttonStyle(.plain)
                    }
                }
                Card {
                    plot(w).padding(.top, scrub == nil ? 12 : 40).padding(.bottom, 4)
                    T.line.frame(height: 1)
                    HStack(spacing: 0) {
                        ForEach([("now", series.now), ("min", w.min), ("avg", w.avg), ("max", w.max)], id: \.0) { k, v in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(k).font(T.font(11)).foregroundStyle(T.mute)
                                Text(v == nil ? "--" : value(series, v) + unit).bold().foregroundStyle(T.fg)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    .padding(.horizontal, 12).padding(.top, 8).padding(.bottom, 10)
                }
                Text((range == .hour ? "every sample" : "mean of every \(range.bucket) min")
                     + (w.missing > 0 ? " · \(w.missing) missing, left blank" : "") + " · hold and drag to read a point")
                    .font(T.font(11)).foregroundStyle(T.mute).padding(.horizontal, 4)
            }
            .padding(.horizontal, 16).padding(.vertical, 12)
            Spacer()
        }
        .font(T.font())
        .background(T.bg)
        .presentationBackground(T.bg)
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
    }

    func plot(_ w: Window) -> some View {
        let top = series.top(w.values)
        let (left, right, above, below) = (34.0, 8.0, 10.0, 22.0)
        return GeometryReader { geo in
            let rect = CGRect(x: left, y: above, width: geo.size.width - left - right,
                              height: geo.size.height - above - below)
            let (t0, t1) = (w.t.first ?? 0, w.t.last ?? 0)
            let x = { (t: Int) in rect.minX + (t1 == t0 ? rect.width : CGFloat(t - t0) / CGFloat(t1 - t0) * rect.width) }
            let y = { (v: Double) in rect.maxY - CGFloat(v / top) * rect.height }
            let label = { (s: String) in Text(s).font(T.font(10)).foregroundStyle(T.mute) }
            Canvas { ctx, size in
                for f in [0, 0.25, 0.5, 0.75, 1] {
                    let v = f * top
                    ctx.stroke(Path { $0.addLines([CGPoint(x: rect.minX, y: y(v)), CGPoint(x: rect.maxX, y: y(v))]) },
                               with: .color(T.line))
                    let text = String(format: "%g", (v * 10).rounded() / 10) + (series.percent && f == 1 ? "%" : "")
                    ctx.draw(label(text), at: CGPoint(x: rect.minX - 6, y: y(v)), anchor: .trailing)
                }
                let step = switch range {
                case .hour: 900
                case .six: 3600
                case .day: 21600
                }
                for tick in stride(from: (t0 + step - 1) / step * step, through: t1, by: step)
                where x(tick) > rect.minX + 14 && x(tick) < rect.maxX - 44 {
                    ctx.draw(label(hhmm(tick)), at: CGPoint(x: x(tick), y: size.height - 6), anchor: .bottom)
                }
                ctx.draw(label("now"), at: CGPoint(x: rect.maxX, y: size.height - 6), anchor: .bottomTrailing)
                trace(ctx, w, top: top, in: rect, dot: scrub == nil)
                guard let i = scrub, let v = w.values[i] else { return }
                let p = CGPoint(x: x(w.t[i]), y: y(v))
                ctx.stroke(Path { $0.addLines([CGPoint(x: p.x, y: rect.minY), CGPoint(x: p.x, y: rect.maxY)]) },
                           with: .color(T.mute))
                let dot = Path(ellipseIn: CGRect(x: p.x - 4, y: p.y - 4, width: 8, height: 8))
                ctx.fill(dot, with: .color(T.accent))
                ctx.stroke(dot, with: .color(T.bg), lineWidth: 2)
                let text = ctx.resolve(Text("\(hhmm(w.t[i]))  \(value(series, v))\(series.percent ? "%" : "")")
                    .font(T.font(12, .bold)).foregroundStyle(T.fg))
                let width = text.measure(in: size).width + 16
                let box = CGRect(x: min(max(p.x - width / 2, rect.minX), rect.maxX - width), y: rect.minY - 30,
                                 width: width, height: 22)
                let shape = Path(roundedRect: box, cornerRadius: 4)
                ctx.fill(shape, with: .color(T.bg))
                ctx.stroke(shape, with: .color(T.edge))
                ctx.draw(text, at: CGPoint(x: box.midX, y: box.midY))
            }
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0)
                .onChanged { g in
                    let at = t0 + Int((g.location.x - rect.minX) / max(rect.width, 1) * CGFloat(t1 - t0))
                    let near = w.t.indices.filter { w.values[$0] != nil }
                        .min { abs(w.t[$0] - at) < abs(w.t[$1] - at) }
                    scrub = near
                }
                .onEnded { _ in scrub = nil })
        }
        .frame(height: 230)
    }
}
