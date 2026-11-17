// Fixture: one MetricsBody (docs/architecture/beacon/metrics.md, "Swift") for host macmini, 1440 samples every 60 s,
// ending at 09:31 today. Shaped like /metrics so impl can turn it into a JSON preview fixture as is.
// Stories: a nightly backup at 03:00 (cpu, load and the backup disk jump), a build 40 min ago, a 4 min gap of
// cpu nulls 22-25 min ago (a failed read), and one boot-time null at the very start.
const N = 1440, EVERY = 60;
const END = (() => { const d = new Date(); d.setHours(9, 31, 0, 0); return Math.floor(d / 1000); })();
let seed = 14;
const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
const bump = (i, at, len, h) => (i >= at && i < at + len ? h * Math.sin(Math.PI * (i - at) / len) : 0);
const GB = 1024 ** 3;

const t = [], cpu = [], l1 = [], l5 = [], l15 = [], mem = [], swap = [], rootU = [], rootF = [], bakU = [], bakF = [];
let la = 1.6;
for (let i = 0; i < N; i++) {
  t.push(END - (N - 1 - i) * EVERY);
  const ago = N - 1 - i;                                   // minutes before 09:31
  const backup = bump(i, N - 391, 50, 70);                 // 03:00 to 03:50
  const build = bump(i, N - 41, 16, 52);                   // 08:50 to 09:06
  const c = 7 + rnd() * 6 + backup + build + (rnd() < .02 ? 20 * rnd() : 0);
  cpu.push(i === 0 || (ago >= 22 && ago < 26) ? null : +c.toFixed(1));
  la = la * .9 + (c / 9) * .1 + (rnd() - .5) * .15;
  l1.push(+la.toFixed(2));
  mem.push(Math.round((15.2 + 1.2 * Math.sin(i / 300) + bump(i, N - 41, 30, 3.1) + rnd() * .3) * GB));
  swap.push(Math.round(.67 * GB));
  const ru = 334.1 * GB + i * 1.6e6;
  rootU.push(Math.round(ru)); rootF.push(Math.round(494.3 * GB - ru));
  const bu = (1.06 + (i >= N - 391 ? .07 * Math.min(1, (i - (N - 391)) / 50) : 0)) * 1024 * GB;
  bakU.push(Math.round(bu)); bakF.push(Math.round(1863 * GB - bu));
}
const ema = (xs, k) => { let v = xs[0]; return xs.map(x => +(v = v + (x - v) / k).toFixed(2)); };
l5.push(...ema(l1, 5)); l15.push(...ema(l1, 15));

export const BODY = {
  version: 1, host: 'macmini', every_s: EVERY, t,
  load1: l1, load5: l5, load15: l15, cpu_pct: cpu,
  mem: { total_bytes: 32 * GB, used_bytes: mem },
  swap: { total_bytes: 2 * GB, used_bytes: swap },
  disks: { '/': { used_bytes: rootU, free_bytes: rootF }, '/Volumes/backup': { used_bytes: bakU, free_bytes: bakF } },
};
