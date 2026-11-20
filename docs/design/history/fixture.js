// Fixtures for the history design (#42): one GET /history body (history.schema.json) from macmini, the config's
// "prober", and one GET /history/<id> body (times.schema.json) for Web/Blog. Same clock as host-detail: 09:31 today.
// Impl can dump both as JSON preview fixtures as they are.
export const END = (() => { const d = new Date(); d.setHours(9, 31, 0, 0); return Math.floor(d / 1000); })();
const H = 3600, D = 86400;
const day0 = (() => { const d = new Date(END * 1000); d.setHours(0, 0, 0, 0); return Math.floor(d / 1000); })();
const at = (daysAgo, h, m) => day0 - daysAgo * D + h * H + m * 60;
export const GAP = [at(6, 2, 10), at(6, 6, 40)];                      // the Mini asleep: neither up nor down

export const HISTORY = {
  version: 1, host: 'macmini', gaps: [GAP],
  checks: {
    backup: { since: END - 30 * D, down: [
      { down_at: at(9, 3, 5), up_at: at(9, 3, 41), message: 'exit 1: rsync: connection reset' },
      { down_at: at(0, 3, 12), up_at: null, message: 'exit 1: rsync: connection reset' }] },
    disk: { since: END - 30 * D, down: [] },
  },
  monitors: [
    { id: '9f2c41d07ab3e815', group: 'Web', name: 'Blog', since: END - 30 * D, down: [
      { down_at: at(12, 14, 2), up_at: at(12, 14, 9), message: 'timeout after 5s' },
      { down_at: at(2, 21, 47), up_at: at(2, 22, 5), message: 'status 502, expected 200' },
      { down_at: at(0, 7, 58), up_at: at(0, 8, 1), message: 'timeout after 5s' }] },
    { id: '4b10e9c27d35a6f0', group: 'Web', name: 'API', since: END - 30 * D, down: [
      { down_at: at(0, 6, 0), up_at: null, message: 'cert expires in 13d' }] },
    { id: 'c3a8f05e1192bd47', group: 'Mail', name: 'smtp', since: at(3, 5, 20), down: [] },
  ],
};

// Blog's times: ~200 ms with a daily swell, timeouts (null) where its incidents were timeouts, fast 502s.
let seed = 42;
const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
const ms = t => {
  const hr = new Date(t * 1000).getHours() + new Date(t * 1000).getMinutes() / 60;
  const swell = 45 * Math.max(0, Math.sin(Math.PI * (hr - 8) / 14));
  const spike = rnd() < .015 ? 150 + 250 * rnd() : 0;
  return Math.round(168 + swell + 28 * rnd() + spike);
};
const inc = HISTORY.monitors[0].down;
const during = t => inc.find(i => t >= i.down_at && t < (i.up_at ?? Infinity));
const asleep = t => t >= GAP[0] && t < GAP[1];
const probe = t => { const i = during(t); return !i ? ms(t) : i.message.startsWith('timeout') ? null : Math.round(38 + 10 * rnd()); };

const rt = [], rms = [];
for (let t = END - 1439 * 60; t <= END; t += 60) { rt.push(t); rms.push(probe(t)); }
const ht = [], avg = [], max = [];
for (let h = Math.floor((END - 719 * H) / H) * H; h <= END; h += H) {
  const v = [];
  for (let t = h; t < h + H && t <= END; t += 60) if (!asleep(t)) { const x = probe(t); if (x != null) v.push(x); }
  if (!v.length) continue;                                            // an hour with no response is absent
  ht.push(h); avg.push(Math.round(v.reduce((a, b) => a + b) / v.length)); max.push(Math.max(...v));
}
export const TIMES = { version: 1, id: '9f2c41d07ab3e815', every_s: 60, recent: { t: rt, ms: rms }, hourly: { t: ht, avg_ms: avg, max_ms: max } };
