// #10: render every PNG with headless Chrome, no deps (cooee's docs/design/app-icons/shoot.mjs, trimmed): each icon
// SVG at 1024 × 1024, the shipped teal AppIcon (q1638), and the home-screen sheet at 390 wide (2×) and 1440 wide (1×).
// Usage (repo root): node app/design/app-icon/art.mjs && node app/design/app-icon/shoot.mjs <chrome-headless-shell>
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { extname, join } from 'node:path';

const here = new URL('.', import.meta.url).pathname;
const [chrome] = process.argv.slice(2);
if (!chrome) { console.error('usage: node shoot.mjs <chrome-headless-shell>'); process.exit(2); }
// [url, width, height, dpr, png]: height 0 means the page's full height
const VIEWS = [
  ['icons/beacon-navy.svg', 1024, 1024, 1, 'beacon-navy-1024.png'],
  ['icons/beacon-teal.svg', 1024, 1024, 1, 'beacon-teal-1024.png'],
  ['icons/beacon-teal.svg', 1024, 1024, 1, '../../Beacon/Assets.xcassets/AppIcon.appiconset/AppIcon.png'],
  ['sheet.html?f=sheet', 390, 0, 2, 'sheet-390.png'],
  ['sheet.html?f=wide', 1440, 0, 1, 'sheet-1440.png'],
];
const TYPES = { '.html': 'text/html', '.svg': 'image/svg+xml' };
const srv = createServer((q, r) => {
  try {
    const p = join(here, decodeURIComponent(new URL(q.url, 'http://x').pathname));
    r.writeHead(200, { 'content-type': TYPES[extname(p)] || 'application/octet-stream' }).end(readFileSync(p));
  } catch { r.writeHead(404).end(); }
}).listen(0, '127.0.0.1');
await new Promise(ok => srv.on('listening', ok));
const base = `http://127.0.0.1:${srv.address().port}/`;

const c = spawn(chrome, ['--headless', '--no-sandbox', '--hide-scrollbars', '--remote-debugging-pipe', '--no-first-run',
  `--user-data-dir=${mkdtempSync(join(tmpdir(), 'beacon-prof-'))}`], { stdio: ['ignore', 'ignore', 'ignore', 'pipe', 'pipe'] });
let buf = '', id = 0;
const wait = new Map();
c.stdio[4].on('data', d => {
  buf += d;
  for (let i; (i = buf.indexOf('\0')) >= 0; buf = buf.slice(i + 1)) {
    const m = JSON.parse(buf.slice(0, i));
    if (m.id && wait.has(m.id)) { wait.get(m.id)(m); wait.delete(m.id); }
  }
});
const send = (method, params = {}, sessionId) => new Promise((ok, no) => {
  wait.set(++id, m => m.error ? no(new Error(`${method}: ${m.error.message}`)) : ok(m.result));
  c.stdio[3].write(JSON.stringify({ id, method, params, sessionId }) + '\0');
});
const sleep = ms => new Promise(ok => setTimeout(ok, ms));

const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId: s } = await send('Target.attachToTarget', { targetId, flatten: true });
const cmd = (m, p) => send(m, p, s);
const js = async x => (await cmd('Runtime.evaluate', { expression: x, returnByValue: true, awaitPromise: true })).result.value;
await cmd('Page.enable');
let fail = 0;
for (const [url, w, h0, dpr, out] of VIEWS) {
  await cmd('Emulation.setDeviceMetricsOverride', { width: w, height: h0 || 844, deviceScaleFactor: dpr, mobile: w < 1000 });
  await cmd('Page.navigate', { url: base + url });
  const html = url.includes('.html');
  for (let i = 0; html && i < 50 && !(await js('document.body && document.body.dataset.ready')); i++) await sleep(100);
  await sleep(300);
  const h = h0 || await js('document.body.scrollHeight');
  const wide = html && await js('document.documentElement.scrollWidth') > w;
  const { data } = await cmd('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true, clip: { x: 0, y: 0, width: w, height: h, scale: 1 } });
  writeFileSync(join(here, out), Buffer.from(data, 'base64'));
  console.log(`${wide ? 'FAIL sideways scroll' : 'ok  '} ${out} ${w * dpr}×${h * dpr}`);
  fail += wide;
}
c.kill(); srv.close();
process.exit(fail ? 1 : 0);
