// Draws Rostery's icon, a ringed planet in the app's night and brass, to a 1024x1024 PNG. It uses the
// browser Playwright already installs, so it needs no new dependency.
//   node scripts/make-icon.mjs && pnpm tauri icon src-tauri/icons/app-icon.png
import { writeFileSync } from 'node:fs';
import { chromium } from '@playwright/test';

const draw = () => {
  const size = 1024;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = size;
  const c = canvas.getContext('2d');

  // A rounded square of night sky, inset the way macOS icons are.
  const inset = 100;
  const side = size - inset * 2;
  const radius = 185;
  c.beginPath();
  c.roundRect(inset, inset, side, side, radius);
  const sky = c.createLinearGradient(0, inset, 0, inset + side);
  sky.addColorStop(0, '#1b2440');
  sky.addColorStop(1, '#0c1019');
  c.fillStyle = sky;
  c.fill();
  c.save();
  c.clip();

  // A few stars, placed by hand so the icon is the same every time.
  const stars = [[230, 260, 3], [310, 690, 2.5], [760, 230, 3.5], [820, 610, 2], [690, 800, 2.5], [250, 470, 2], [520, 200, 2]];
  c.fillStyle = 'rgba(236, 232, 218, 0.8)';
  for (const [x, y, r] of stars) {
    c.beginPath();
    c.arc(x, y, r, 0, Math.PI * 2);
    c.fill();
  }

  const cx = 512;
  const cy = 530;
  const planet = 205;
  const tilt = -0.42;
  const ring = (front) => {
    c.save();
    c.translate(cx, cy);
    c.rotate(tilt);
    c.beginPath();
    // The front half of the ring passes over the planet, and the back half behind it.
    c.ellipse(0, 0, 355, 92, 0, front ? 0 : Math.PI, front ? Math.PI : Math.PI * 2);
    c.lineWidth = 34;
    c.strokeStyle = '#d6a854';
    c.stroke();
    c.lineWidth = 8;
    c.strokeStyle = 'rgba(12, 16, 25, 0.55)';
    c.stroke();
    c.restore();
  };

  ring(false);

  // The planet, lit from the upper left, with soft bands.
  c.save();
  c.beginPath();
  c.arc(cx, cy, planet, 0, Math.PI * 2);
  c.clip();
  const body = c.createRadialGradient(cx - 80, cy - 90, 20, cx, cy, planet);
  body.addColorStop(0, '#f3d79b');
  body.addColorStop(0.55, '#d6a854');
  body.addColorStop(1, '#7a5520');
  c.fillStyle = body;
  c.fillRect(cx - planet, cy - planet, planet * 2, planet * 2);
  c.translate(cx, cy);
  c.rotate(tilt);
  for (const [y, h, a] of [[-120, 26, 0.22], [-55, 38, 0.16], [25, 22, 0.24], [85, 46, 0.14]]) {
    c.fillStyle = `rgba(90, 58, 20, ${a})`;
    c.fillRect(-planet, y, planet * 2, h);
  }
  c.restore();

  // The night side.
  c.beginPath();
  c.arc(cx, cy, planet, 0, Math.PI * 2);
  const shade = c.createLinearGradient(cx - planet, cy - planet, cx + planet, cy + planet);
  shade.addColorStop(0.45, 'rgba(12, 16, 25, 0)');
  shade.addColorStop(1, 'rgba(12, 16, 25, 0.6)');
  c.fillStyle = shade;
  c.fill();

  ring(true);
  c.restore();
  return canvas.toDataURL('image/png');
};

const browser = await chromium.launch();
const page = await browser.newPage();
const url = await page.evaluate(draw);
await browser.close();
const out = new URL('../src-tauri/icons/app-icon.png', import.meta.url);
writeFileSync(out, Buffer.from(url.split(',')[1], 'base64'));
console.log(`wrote ${out.pathname}`);
