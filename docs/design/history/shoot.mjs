// #42: render every PNG with headless Chrome, no deps (app/design/app-icon/shoot.mjs with these views). It serves the
// repo root, so states.html can show the shipped icon art. 390 wide at 3× (iPhone), 1440 wide at 1×.
// Usage (repo root): node docs/design/history/shoot.mjs <chrome-headless-shell>
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { extname, join } from 'node:path';

const here = new URL('.', import.meta.url).pathname, root = join(here, '../../..'), dir = 'docs/design/history/';
const [chrome] = process.argv.slice(2);
if (!chrome) { console.error('usage: node shoot.mjs <chrome-headless-shell>'); process.exit(2); }
// [page, width, dpr, png]; the height is always the page's own
const VIEWS = [];
for (const m of ['dark', 'light']) VIEWS.push(
  [`monitor.html?m=${m}&r=24h`, 390, 3, `monitor-24h-${m}.png`],
  [`monitor.html?m=${m}&r=1h&scrub=24`, 390, 3, `monitor-1h-${m}.png`],
  [`monitor.html?m=${m}&r=30d`, 390, 3, `monitor-30d-${m}.png`],
  [`check.html?m=${m}`, 390, 3, `check-${m}.png`],
  [`states.html?m=${m}`, 390, 3, `states-${m}.png`]);
VIEWS.push(['monitor.html?f=wide&r=24h', 1440, 1, 'monitor-1440.png'], ['check.html?f=wide', 1440, 1, 'check-1440.png'],
  ['states.html?f=wide', 1440, 1, 'states-1440.png']);
mkdirSync(join(here, 'png'), { recursive: true });
const TYPES = { '.html': 'text/html', '.svg': 'image/svg+xml', '.js': 'text/javascript', '.css': 'text/css' };
const srv = createServer((q, r) => {
  try {
    const p = join(root, decodeURIComponent(new URL(q.url, 'http://x').pathname));
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
for (const [page, w, dpr, out] of VIEWS) {
  await cmd('Emulation.setDeviceMetricsOverride', { width: w, height: 844, deviceScaleFactor: dpr, mobile: w < 1000 });
  await cmd('Page.navigate', { url: base + dir + page });
  const html = true;
  for (let i = 0; html && i < 50 && !(await js('document.body && document.body.dataset.ready')); i++) await sleep(100);
  await sleep(300);
  const h = await js('document.body.scrollHeight');
  const wide = html && await js('document.documentElement.scrollWidth') > w;
  const { data } = await cmd('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true, clip: { x: 0, y: 0, width: w, height: h, scale: 1 } });
  writeFileSync(join(here, 'png', out), Buffer.from(data, 'base64'));
  console.log(`${wide ? 'FAIL sideways scroll' : 'ok  '} ${out} ${w * dpr}×${h * dpr}`);
  fail += wide;
}
c.kill(); srv.close();
process.exit(fail ? 1 : 0);
