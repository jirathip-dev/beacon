// Preview-only drawing for the history design (#42). No fetching: every screen draws fixture.js.
import { HISTORY, TIMES, END } from './fixture.js';
export { statusBar, mount } from '../host-detail/view.js';

const D = 86400;
const hm = s => new Date(s * 1000).toTimeString().slice(0, 5);
const today = s => new Date(s * 1000).toDateString() === new Date(END * 1000).toDateString();
const md = s => new Date(s * 1000).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
// T.stamp's rule, with the time kept: today "HH:mm", else "Nov 17 HH:mm".
export const stamp = s => (today(s) ? hm(s) : `${md(s)} ${hm(s)}`);
export const dur = s => { const m = Math.round(s / 60), h = Math.floor(m / 60), d = Math.floor(h / 24);
  return d ? `${d}d ${h % 24}h` : h ? `${h}h ${m % 60}m` : `${Math.max(1, m)}m`; };

const overlap = (a0, a1, b0, b1) => Math.max(0, Math.min(a1, b1) - Math.max(a0, b0));
// history.md "Uptime of a key over the last W seconds", worked out by the app.
export function uptime(track, W, gaps = HISTORY.gaps, now = END) {
  const w0 = Math.max(now - W, track.since), span = now - w0;
  const unknown = gaps.reduce((s, [a, b]) => s + overlap(a, b, w0, now), 0);
  const down = track.down.reduce((s, i) => { const e = i.up_at ?? now;
    return s + overlap(i.down_at, e, w0, now) - gaps.reduce((g, [a, b]) => g + overlap(a, b, Math.max(i.down_at, w0), Math.min(e, now)), 0); }, 0);
  const watched = span - unknown;
  return { watched, span, up: watched > 0 ? 1 - down / watched : null, down };
}
// Floor, never round up: 100% means no down second at all.
export const pct = u => (u == null ? '--' : u === 1 ? '100%' : `${(Math.floor(u * 1e4) / 100).toFixed(2)}%`);

// 30 day cells, oldest first: red a day with down time, green a watched day without, mute not watched, blank before since.
export function strip(track) {
  const end = new Date(END * 1000); end.setHours(0, 0, 0, 0);
  const cells = [];
  for (let k = 29; k >= 0; k--) {
    const a = end / 1000 - k * D, b = Math.min(a + D, END);
    const u = uptime({ ...track, since: Math.max(track.since, a) }, b - a, HISTORY.gaps, b);
    const cls = a + D <= track.since ? 'none' : u.watched <= 0 ? 'gap' : u.down > 0 ? 'bad' : 'ok';
    cells.push(`<i class="${cls}" title="${md(a)}"></i>`);
  }
  return `<div class="strip" role="img" aria-label="last 30 days, a cell a day">${cells.join('')}</div>
    <div class="ends lab"><span>${md(END - 29 * D)}</span><span>today</span></div>`;
}

export function uptimeCard(track, title = '~ uptime') {
  const d1 = uptime(track, D), d30 = uptime(track, 30 * D);
  const label = (u, w) => (u.span < w - 60 ? `${dur(u.span)}, all it has` : w === D ? 'last 24h' : 'last 30d');
  const gapLine = HISTORY.gaps.filter(([a, b]) => b > Math.max(END - 30 * D, track.since)).map(([a, b]) => `${dur(b - a)} not watched on ${md(a)}`);
  return `<div class="card"><div class="hd"><span>${title}</span><span class="mute">as seen from ${HISTORY.host}</span></div>
    <div class="ups"><div><span class="lab">${label(d1, D)}</span><b class="big">${pct(d1.up)}</b></div>
      <div><span class="lab">${label(d30, 30 * D)}</span><b class="big">${pct(d30.up)}</b></div></div>
    <div class="pad">${strip(track)}</div>
    <p class="meta mute pad2">watched ${dur(d30.watched)}${gapLine.length ? ` · ${gapLine.join(' · ')}` : ''}</p></div>`;
}

export function incidents(track, n = 5) {
  const all = track.down.filter(i => (i.up_at ?? END) > END - 30 * D).reverse();
  const row = i => i.up_at == null
    ? `<div class="inc open"><span class="line1 red"><span class="dot"></span>down since ${stamp(i.down_at)}</span><span class="red">${dur(END - i.down_at)} so far</span>
        <span class="meta mute msg">${i.message}</span></div>`
    : `<div class="inc"><span class="line1"><span class="dot red"></span>${stamp(i.down_at)} → ${today(i.up_at) === today(i.down_at) && md(i.up_at) === md(i.down_at) ? hm(i.up_at) : stamp(i.up_at)}</span><span>${dur(i.up_at - i.down_at)}</span>
        <span class="meta mute msg">${i.message}</span></div>`;
  return `<div class="card"><div class="hd"><span>~ incidents</span><span class="mute">${all.length ? `${Math.min(n, all.length)} of ${all.length} · 30d` : 'none in 30d'}</span></div>
    ${all.length ? all.slice(0, n).map(row).join('') : `<div class="word" style="padding:18px 16px"><div class="green">● clear</div><div class="mute">not down once while watched</div></div>`}</div>`;
}

// The response-time chart: 1h and 24h from `recent`, 30d from `hourly` (avg as the line, max as a dashed mute line).
const nice = v => [40, 80, 100, 200, 400, 800, 1000, 2000, 4000, 8000].find(n => n >= v) ?? Math.ceil(v / 4000) * 4000;
const RANGES = { '1h': [3600, 15 * 60], '24h': [D, 6 * 3600], '30d': [30 * D, 7 * D] };
function series(range) {
  const [W] = RANGES[range], from = END - W;
  if (range === '30d') { const h = TIMES.hourly, k = h.t.map((t, i) => i).filter(i => h.t[i] >= from);
    return { t: k.map(i => h.t[i]), y: k.map(i => h.avg_ms[i]), max: k.map(i => h.max_ms[i]), miss: [] }; }
  const r = TIMES.recent, k = r.t.map((t, i) => i).filter(i => r.t[i] > from);
  return { t: k.map(i => r.t[i]), y: k.map(i => r.ms[i]), miss: k.filter(i => r.ms[i] == null).map(i => r.t[i]) };
}
function bucket(ts, ys, n, how = 'mean') {                                   // per bucket; all-null stays a gap
  const k = Math.max(1, Math.ceil(ys.length / n)), T = [], Y = [];
  for (let i = 0; i < ys.length; i += k) { const v = ys.slice(i, i + k).filter(x => x != null);
    T.push(ts[Math.min(i + k - 1, ts.length - 1)]);
    Y.push(!v.length ? null : how === 'max' ? Math.max(...v) : v.reduce((a, b) => a + b) / v.length); }
  return [T, Y];
}
function line(ts, ys, x, y, cls, gapS) {                                      // breaks at a null and at a hole in time
  let d = '', pen = false;
  ys.forEach((v, i) => { if (v == null || (i && ts[i] - ts[i - 1] > gapS)) pen = false;
    if (v != null) { d += `${pen ? 'L' : 'M'}${x(ts[i]).toFixed(1)},${y(v).toFixed(1)}`; pen = true; } });
  return `<path class="${cls}" d="${d}"/>`;
}
export function chart(range, scrubAgo = null) {
  const W = 356, H = 210, L = 40, R = 8, TOP = 10, BOT = 22, pw = W - L - R, ph = H - TOP - BOT;
  const s = series(range), t0 = END - RANGES[range][0], t1 = END;
  const [bt, by] = bucket(s.t, s.y, range === '30d' ? 180 : pw / 1.5);
  const bm = range === '30d' ? bucket(s.t, s.max, 180, 'max')[1] : null;
  const hi = nice(Math.max(...(bm ?? by).filter(v => v != null)) * 1.05);                // the plotted line fits; max in stats is raw
  const x = t => L + ((t - t0) / (t1 - t0)) * pw, y = v => TOP + ph - (Math.min(v, hi) / hi) * ph;
  let g = [0, .25, .5, .75, 1].map(f => f * hi).map(v => `<line class="grid" x1="${L}" x2="${W - R}" y1="${y(v)}" y2="${y(v)}"/>
    <text class="ax" x="${L - 6}" y="${y(v) + 3}" text-anchor="end">${v}${v === hi ? 'ms' : ''}</text>`).join('');
  const step = RANGES[range][1];
  for (let t = range === '30d' ? t1 - step : Math.ceil(t0 / step) * step; range === '30d' ? t > t0 : t <= t1; t += range === '30d' ? -step : step)
    if (x(t) > L + 14 && x(t) < W - R - 44) g += `<text class="ax" x="${x(t)}" y="${H - 6}" text-anchor="middle">${range === '30d' ? md(t) : hm(t)}</text>`;
  g += `<text class="ax" x="${W - R}" y="${H - 6}" text-anchor="end">now</text>`;
  const gapS = 1.5 * (range === '30d' ? 3600 * Math.ceil(s.y.length / 180) : 60 * Math.ceil(s.y.length / (pw / 1.5)));
  let p = '';
  if (bm) {                                                                   // the band from avg up to max, broken at holes
    let run = [];
    const flush = () => { if (run.length) p += `<path class="ar" d="M${run.map(i => `${x(bt[i]).toFixed(1)},${y(bm[i]).toFixed(1)}`).join('L')}L${run.slice().reverse().map(i => `${x(bt[i]).toFixed(1)},${y(by[i]).toFixed(1)}`).join('L')}Z"/>`; run = []; };
    by.forEach((v, i) => { if (v == null || (i && bt[i] - bt[i - 1] > gapS)) flush(); if (v != null) run.push(i); }); flush();
  }
  p += line(bt, by, x, y, 'ln', gapS);
  p += s.miss.map(t => `<line class="miss" x1="${x(t)}" x2="${x(t)}" y1="${TOP + ph - 7}" y2="${TOP + ph}"/>`).join('');
  let sc = '';
  if (scrubAgo != null) {
    const i = s.t.length - 1 - scrubAgo, v = s.y[i], cx = x(s.t[i]), lab = `${hm(s.t[i])}  ${v == null ? 'no response' : v + 'ms'}`;
    const bw = lab.length * 7.4 + 16, bx = Math.min(Math.max(cx - bw / 2, L), W - R - bw);
    sc = `<line x1="${cx}" x2="${cx}" y1="${TOP}" y2="${TOP + ph}" stroke="var(--mute)"/>${v == null ? '' : `<circle class="now" r="4" cx="${cx}" cy="${y(v)}" style="stroke:var(--card)"/>`}
      <rect x="${bx}" y="${TOP - 30}" width="${bw}" height="22" rx="4" fill="var(--bg)" stroke="var(--edge)"/>
      <text x="${bx + bw / 2}" y="${TOP - 15}" text-anchor="middle" fill="var(--fg)" font-size="12" font-family="inherit" font-weight="700">${lab}</text>`;
  } else { const k = by.findLastIndex(v => v != null); p += `<circle class="now" r="3.5" cx="${x(bt[k])}" cy="${y(by[k])}"/>`; }
  return `<svg width="${W}" height="${H}" role="img" aria-label="response time, last ${range}">${g}${p}${sc}</svg>`;
}
export function stats(range) {
  const s = series(range), v = s.y.filter(x => x != null), mx = (s.max ?? s.y).filter(x => x != null);
  const now = TIMES.recent.ms.findLast(x => x != null);
  return [['now', now], ['min', Math.min(...v)], ['avg', Math.round(v.reduce((a, b) => a + b) / v.length)], ['max', Math.max(...mx)]]
    .map(([k, x]) => `<div><span class="lab">${k}</span><b>${x}<span class="lab">ms</span></b></div>`).join('');
}
export const missing = range => series(range).miss.length;

export const top = (back, title, sub) => `<div class="top"><span class="key">[‹ ${back}]</span><span class="ttl"><span class="title">${title}</span>${sub ? `<span class="lab">${sub}</span>` : ''}</span><span style="width:96px"></span></div>`;
export { HISTORY, TIMES, END, md };
