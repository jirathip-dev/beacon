// #10: Beacon's app icon, drawn in the look of cooee's illustrated app icons (jirathip-dev/cooee
// docs/design/app-icons/art.mjs): flat fills, soft shading (black or white at low opacity), one solid dark ground,
// 1024 × 1024, a close crop. The subject is a lighthouse, head and shoulders: the dome, the lamp room and the gallery,
// the top of a red-and-white tower, and a warm beam out to each side. Drawn on two grounds (q1607); one ships (q1610).
// Writes icons/beacon-<ground>.svg. Usage: node app/design/app-icon/art.mjs
import { writeFileSync } from 'node:fs';

const here = new URL('.', import.meta.url).pathname;
const shade = (d, o = 0.14) => `<path d="${d}" fill="#000" opacity="${o}"/>`;
const light = (d, o = 0.16) => `<path d="${d}" fill="#fff" opacity="${o}"/>`;
const svg = (bg, body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><rect width="1024" height="1024" fill="${bg}"/>${body}</svg>\n`;

// The two grounds the owner chooses between (q1607): cooee Modern's night-sea navy, and a deep teal.
export const GROUNDS = { navy: '#13263d', teal: '#12303a' };
// Red-and-white tower, a warm yellow lamp and beams (q1604); the red is cooee Kitchen's line-cook bandana.
const C = { red: '#c8453a', redDark: '#9e3129', white: '#f1ece2', iron: '#262a31', glass: '#f6c94a', lamp: '#fff4c8', beam: '#ffe08a' };

const CX = 512, LAMP_Y = 455; // the lamp's centre: both beams start here

// A beam: a wedge from the lamp to the edge of the icon, one per side, a soft outer cone and a brighter core.
const beam = side => {
  const x = side < 0 ? 0 : 1024;
  return `<path d="M${CX} ${LAMP_Y - 34} L${x} ${LAMP_Y - 200} L${x} ${LAMP_Y + 170} L${CX} ${LAMP_Y + 34} Z" fill="${C.beam}" opacity=".18"/>
  <path d="M${CX} ${LAMP_Y - 20} L${x} ${LAMP_Y - 100} L${x} ${LAMP_Y + 76} L${CX} ${LAMP_Y + 20} Z" fill="${C.beam}" opacity=".7"/>`;
};

// The tower below the gallery: a tapering cone, red-white-red, cropped by the bottom edge like cooee's cooks.
const TOP = 640, L0 = 360, R0 = 664, L1 = 286, R1 = 738; // x at y = TOP and at y = 1024
const xAt = (edge0, edge1, y) => edge0 + ((edge1 - edge0) * (y - TOP)) / (1024 - TOP);
const band = (y0, y1, fill) =>
  `<path d="M${xAt(L0, L1, y0)} ${y0} L${xAt(R0, R1, y0)} ${y0} L${xAt(R0, R1, y1)} ${y1} L${xAt(L0, L1, y1)} ${y1} Z" fill="${fill}"/>`;
const tower = `
  ${band(TOP, 790, C.red)}${band(790, 920, C.white)}${band(920, 1024, C.red)}
  ${shade(`M${CX + 70} ${TOP} L${R0} ${TOP} L${R1} 1024 L${CX + 96} 1024 Z`, 0.16)}
  ${light(`M${L0 + 20} ${TOP} L${L0 + 74} ${TOP} L${L1 + 84} 1024 L${L1 + 24} 1024 Z`, 0.1)}
  <path d="M482 900 L482 862 C482 838 542 838 542 862 L542 900 Z" fill="${C.iron}"/>`;

// The gallery: a deck that juts past the tower; its railing is drawn after the lamp room, in front of the glass.
const gallery = `
  <path d="M296 612 L728 612 L700 652 L324 652 Z" fill="${C.redDark}"/>
  <rect x="276" y="594" width="472" height="30" rx="10" fill="${C.iron}"/>`;
const railing = `
  <rect x="300" y="526" width="424" height="16" rx="8" fill="${C.iron}"/>
  <g fill="${C.iron}">${[316, 368, 420, 472, 552, 604, 656, 708].map(x => `<rect x="${x - 6}" y="532" width="12" height="66"/>`).join('')}</g>`;

// The lamp room: warm glass in iron glazing bars, the lamp itself bright in the middle.
const lantern = `
  <rect x="392" y="356" width="240" height="246" fill="${C.glass}"/>
  <circle cx="${CX}" cy="${LAMP_Y}" r="76" fill="${C.lamp}"/>
  <circle cx="${CX}" cy="${LAMP_Y}" r="40" fill="#fff"/>
  ${shade('M572 356 L632 356 L632 602 L572 602 Z', 0.12)}
  <g fill="${C.iron}"><rect x="384" y="350" width="24" height="256"/><rect x="616" y="350" width="24" height="256"/>
    <rect x="452" y="356" width="12" height="244" opacity=".85"/><rect x="560" y="356" width="12" height="244" opacity=".85"/></g>`;

// The dome: a red cap on an iron cornice, a vent ball and a short finial.
const dome = `
  <rect x="360" y="336" width="304" height="34" rx="10" fill="${C.iron}"/>
  <path d="M378 344 C384 258 446 214 512 210 C578 214 640 258 646 344 Z" fill="${C.red}"/>
  ${shade('M512 210 C578 214 640 258 646 344 L560 344 C566 286 548 236 512 210 Z', 0.16)}
  ${light('M410 330 C416 280 446 246 486 230 C456 260 440 296 438 332 Z', 0.2)}
  <circle cx="${CX}" cy="200" r="26" fill="${C.iron}"/><rect x="${CX - 7}" y="140" width="14" height="50" rx="7" fill="${C.iron}"/>`;

// A close crop like cooee's cooks: the drawing grows 12% about the bottom centre, so the finial nears the top edge.
const lighthouse = bg =>
  svg(bg, `<g transform="translate(${CX} 1024) scale(1.12) translate(${-CX} -1024)">${beam(-1)}${beam(1)}${tower}${gallery}${lantern}${railing}${dome}</g>`);

for (const [name, bg] of Object.entries(GROUNDS)) writeFileSync(`${here}icons/beacon-${name}.svg`, lighthouse(bg));
console.log(Object.keys(GROUNDS).length, 'icons written');
