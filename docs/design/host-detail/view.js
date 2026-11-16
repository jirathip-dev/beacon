// Preview-only drawing for the host detail design (#14). No fetching: every screen draws fixture.js.
import { BODY } from './fixture.js';

const GB = 1024 ** 3;
const q = new URLSearchParams(location.search);
const pct = (u, tot) => (u == null || tot == null ? null : (100 * u) / tot);
const last = xs => { for (let i = xs.length - 1; i >= 0; i--) if (xs[i] != null) return xs[i]; return null; };
const gb = b => (b / GB >= 100 || Number.isInteger(b / GB) ? (b / GB).toFixed(0) : (b / GB).toFixed(1));
const hhmm = s => new Date(s * 1000).toTimeString().slice(0, 5);

// The metrics, in screen order. `scale` is the chart's y range: percentages are 0-100 always, load is 0 to a nice max.
const B = BODY;
const disk = m => B.disks[m].used_bytes.map((u, i) => pct(u, u == null || B.disks[m].free_bytes[i] == null ? null : u + B.disks[m].free_bytes[i]));
const METRICS = [
  { id: 'load', label: 'load', ys: B.load1, fmt: v => v.toFixed(2), unit: '',
    sub: () => `5m ${last(B.load5).toFixed(2)} · 15m ${last(B.load15).toFixed(2)}` },
  { id: 'cpu', label: 'cpu', ys: B.cpu_pct, fmt: v => v.toFixed(1), unit: '%', pct: true, sub: () => 'all cores' },
  { id: 'mem', label: 'mem', ys: B.mem.used_bytes.map(u => pct(u, B.mem.total_bytes)), fmt: v => v.toFixed(0), unit: '%', pct: true,
    sub: () => `${gb(last(B.mem.used_bytes))} of ${gb(B.mem.total_bytes)} GB · swap ${gb(last(B.swap.used_bytes))}` },
  ...Object.keys(B.disks).map(m => ({ id: 'disk' + m, label: 'disk ' + m, ys: disk(m), fmt: v => v.toFixed(0), unit: '%', pct: true,
    sub: () => { const u = last(B.disks[m].used_bytes), f = last(B.disks[m].free_bytes); return `${gb(f)} of ${gb(u + f)} GB free`; } })),
];
const nice = v => [1, 2, 4, 5, 8, 10, 16, 20, 40, 50, 100].find(n => n >= v) ?? Math.ceil(v);
const top = (m, ys) => (m.pct ? 100 : nice(Math.max(1, ...ys.filter(v => v != null))));

// Mean per bucket so a range always fits the plot width; an all-null bucket stays a gap.
function bucket(ts, ys, n) {
  const k = Math.max(1, Math.ceil(ys.length / n)), T = [], Y = [];
  for (let i = 0; i < ys.length; i += k) {
    const v = ys.slice(i, i + k).filter(x => x != null);
    T.push(ts[Math.min(i + k - 1, ts.length - 1)]); Y.push(v.length ? v.reduce((a, b) => a + b) / v.length : null);
  }
  return [T, Y];
}
// A line that breaks at every null, and its area to the baseline.
function paths(xs, ys, x, y, y0) {
  let ln = '', ar = '', run = [];
  const flush = () => {
    if (!run.length) return;
    ln += run.map(([a, b], j) => `${j ? 'L' : 'M'}${a.toFixed(1)},${b.toFixed(1)}`).join('');
    ar += `M${run[0][0].toFixed(1)},${y0}` + run.map(([a, b]) => `L${a.toFixed(1)},${b.toFixed(1)}`).join('') + `L${run.at(-1)[0].toFixed(1)},${y0}Z`;
    run = [];
  };
  ys.forEach((v, i) => (v == null ? flush() : run.push([x(xs[i]), y(v)])));
  flush();
  return `<path class="ar" d="${ar}"/><path class="ln" d="${ln}"/>`;
}
const tail = (m, mins) => [B.t.slice(-mins), m.ys.slice(-mins)];

export function spark(m, w = 132, h = 36) {
  const [ts, ys] = tail(m, 60), hi = top(m, m.ys.slice(-60)), t0 = ts[0], t1 = ts.at(-1);
  const x = t => ((t - t0) / (t1 - t0)) * w, y = v => h - (v / hi) * h;
  const lv = last(ys), li = ys.lastIndexOf(lv);
  return `<svg width="${w}" height="${h}" aria-hidden="true"><line class="grid" x1="0" x2="${w}" y1="${h}" y2="${h}"/>
    ${paths(ts, ys, x, y, h)}${lv == null ? '' : `<circle class="now" r="3" cx="${x(ts[li])}" cy="${y(lv)}"/>`}</svg>`;
}

const RANGES = { '1h': [60, 15 * 60, 4], '6h': [360, 3600, 2], '24h': [1440, 6 * 3600, 1] }; // samples, tick step, -
export function chart(m, range, scrubAt) {
  const W = 356, H = 230, L = 34, R = 8, TOP = 10, BOT = 22, pw = W - L - R, ph = H - TOP - BOT;
  const [rt, ry] = tail(m, RANGES[range][0]);
  const [ts, ys] = bucket(rt, ry, pw / 1.5);
  const hi = top(m, ry), t0 = rt[0], t1 = rt.at(-1);
  const x = t => L + ((t - t0) / (t1 - t0)) * pw, y = v => TOP + ph - (v / hi) * ph;
  const ticksY = [0, .25, .5, .75, 1].map(f => f * hi);
  let g = ticksY.map(v => `<line class="grid" x1="${L}" x2="${W - R}" y1="${y(v)}" y2="${y(v)}"/>
    <text class="ax" x="${L - 6}" y="${y(v) + 3}" text-anchor="end">${m.pct ? v : +v.toFixed(1)}${m.pct && v === 100 ? '%' : ''}</text>`).join('');
  const step = RANGES[range][1];
  for (let t = Math.ceil(t0 / step) * step; t <= t1; t += step)
    if (x(t) > L + 14 && x(t) < W - R - 44) g += `<text class="ax" x="${x(t)}" y="${H - 6}" text-anchor="middle">${hhmm(t)}</text>`;
  g += `<text class="ax" x="${W - R}" y="${H - 6}" text-anchor="end">now</text>`;
  let s = '';
  if (scrubAt != null) {
    const i = rt.length - 1 - scrubAt, v = ry[i], cx = x(rt[i]), lab = `${hhmm(rt[i])}  ${m.fmt(v)}${m.unit}`;
    const bw = lab.length * 7.4 + 16, bx = Math.min(Math.max(cx - bw / 2, L), W - R - bw);
    s = `<line x1="${cx}" x2="${cx}" y1="${TOP}" y2="${TOP + ph}" stroke="var(--mute)" stroke-width="1"/>
      <circle class="now" r="4" cx="${cx}" cy="${y(v)}" style="stroke:var(--bg)"/>
      <rect x="${bx}" y="${TOP - 30}" width="${bw}" height="22" rx="4" fill="var(--bg)" stroke="var(--edge)"/>
      <text x="${bx + bw / 2}" y="${TOP - 15}" text-anchor="middle" fill="var(--fg)" font-size="12" font-family="inherit" font-weight="700">${lab}</text>`;
  }
  const lv = last(ys);
  const dot = scrubAt == null && lv != null ? `<circle class="now" r="3.5" cx="${x(ts[ys.lastIndexOf(lv)])}" cy="${y(lv)}"/>` : '';
  return `<svg width="${W}" height="${H}" role="img" aria-label="${m.label}, last ${range}">${g}${paths(ts, ys, x, y, TOP + ph)}${dot}${s}</svg>`;
}

export function stats(m, range) {
  const v = m.ys.slice(-RANGES[range][0]).filter(x => x != null);
  const f = x => `${m.fmt(x)}${m.unit}`;
  return [['now', last(m.ys)], ['min', Math.min(...v)], ['avg', v.reduce((a, b) => a + b) / v.length], ['max', Math.max(...v)]]
    .map(([k, x]) => `<div><span class="lab">${k}</span><b>${f(x)}</b></div>`).join('');
}
export const missing = (m, range) => m.ys.slice(-RANGES[range][0]).filter(x => x == null).length;

export const rows = (dim = false) => METRICS.map(m => `<div class="row">
  <div><div class="lab">${m.label}</div><div class="big ${dim ? 'mute' : ''}">${m.fmt(last(m.ys))}<span class="lab">${m.unit}</span></div>
  <div class="lab">${m.sub()}</div></div><div class="${dim ? 'dim' : ''}">${spark(m)}</div><span class="chev">›</span></div>`).join('');

export { METRICS, B, hhmm };

export const statusBar = () => `<div class="sb"><span>9:31</span><span>●●● ▮</span></div>`;

// Each page draws `phone()` once at 390 (mode from ?m=), or dark and light side by side when wide (?f=wide).
export function mount(phone, title, captions = []) {
  const wide = q.get('f') === 'wide';
  const one = mode => `<div><p class="cap">${mode}</p>${[].concat(phone(mode)).map((p, i) =>
    `<div style="margin-bottom:28px">${captions[i] ? `<p class="cap">${captions[i]}</p>` : ''}<div class="phone ${mode}"${captions.length ? ' style="min-height:0;padding-bottom:12px"' : ''}>${p}</div></div>`).join('')}</div>`;
  if (wide) document.body.innerHTML = `<div class="sheet"><h1>${title}</h1>${one('dark')}${one('light')}</div>`;
  else {
    const mode = q.get('m') || 'dark';
    document.body.className = 'one ' + mode;
    document.body.innerHTML = [].concat(phone(mode)).map((p, i) =>
      `${captions[i] ? `<p class="cap" style="padding:10px 16px 0;color:var(--mute)">${captions[i]}</p>` : ''}<div class="phone ${mode}"${captions.length ? ' style="min-height:0;padding-bottom:12px;border-bottom:6px solid var(--edge)"' : ''}>${p}</div>`).join('');
  }
  document.body.dataset.ready = '1';
}
