## Summary

Decision: Beacon's icon is a lighthouse drawn head-and-shoulders in Cooee's illustrated style: a red-and-white tower, a warm yellow lamp room and a beam out each side, on one solid dark ground. The drawing is the same in both variants; only the ground differs.
Screens: the icon on night-sea navy and on deep teal at 1024, and a home-screen sheet (390 and 1440) with both among Cooee's icons on a dark and a light wallpaper.
Open questions: which ground ships, navy `#13263d` or teal `#12303a`? Recommendation: navy. It is the darker of the two and fits a night sea, so the yellow beams stand out most. Teal sits closer to Cooee's Garden green.

## Images and the decision each shows

| image | shows | decision |
|---|---|---|
| `beacon-navy-1024.png` | The icon on night-sea navy `#13263d` (Cooee Modern's ground), 1024 × 1024. | Ground option 1 (q1607). Subject and crop (q1604): the dome, the lamp room behind its railing, and the top of the tower, which the bottom edge cuts off as it does Kitchen's cooks. |
| `beacon-teal-1024.png` | The same drawing on deep teal `#12303a`, 1024 × 1024. | Ground option 2 (q1607). |
| `sheet-390.png` | Both variants at home-screen size (60 pt) in the first row, each beside one of Cooee's icons (Laughing, Monstera), then Head chef, Roo, Sunflower and Koala. Shown on a dark and a light wallpaper. | Style (q1603): a flat picture on one dark ground that sits in the pack. It reads at 60 pt: the red cap, yellow lamp and beams hold without the detail. |
| `sheet-1440.png` | Both variants at 240 pt with their hex codes, beside the same two wallpapers. | For comparing the two grounds side by side. |

## Files

| file | what |
|---|---|
| `art.mjs` | Draws `icons/beacon-navy.svg` and `icons/beacon-teal.svg` (1024 × 1024): `node app/design/app-icon/art.mjs`. It follows Cooee's `art.mjs` method: flat fills, black or white shading at low opacity, one ground rect. Colours: red `#c8453a` (Cooee Kitchen's line-cook red), white `#f1ece2`, lamp `#f6c94a`/`#fff4c8`, beams `#ffe08a`, iron `#262a31`. |
| `icons/` | The two SVGs. |
| `ref/` | Seven of Cooee's icons, copied unchanged from `jirathip-dev/cooee` `docs/design/app-icons/icons/` (art.mjs at `a4f7fa99`). They appear only on the comparison sheet and do not ship. |
| `sheet.html` | The home-screen sheet: `?f=sheet` (390) or `?f=wide` (1440). Its wallpapers, squircle and 60 pt grid copy Cooee's `ai.css`. |
| `shoot.mjs` | Renders every PNG with headless Chrome, with no dependencies: `node app/design/app-icon/shoot.mjs <chrome-headless-shell>`. It fails if a page scrolls sideways. |

## For the impl lane (not drawn)

- One icon ships (q1610): the chosen variant becomes `AppIcon.appiconset`'s single 1024 × 1024 image. There are no alternates.
- Render the PNG from the committed SVG with the committed script, with no hand edits. iOS needs an opaque PNG, but Chrome's screenshot is RGBA. Flatten it, or render without alpha, so the shipped PNG has no alpha channel.
