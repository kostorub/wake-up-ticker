'use strict';

/**
 * Generates every PNG the app needs, with no image dependencies.
 * Shapes are described as signed-distance fields and rasterised with
 * 4x4 supersampling, then written out as raw PNG (zlib is built in).
 *
 *   node scripts/generate-icons.js
 */

const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

/* ------------------------------------------------------------------ PNG */

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

function encodePng(size, rgba) {
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (stride + 1)] = 0; // filter: none
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // colour type: RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

/* --------------------------------------------------------------- shapes */

function segmentDistance(px, py, ax, ay, bx, by) {
  const vx = bx - ax;
  const vy = by - ay;
  const wx = px - ax;
  const wy = py - ay;
  const len2 = vx * vx + vy * vy;
  let t = len2 === 0 ? 0 : (wx * vx + wy * vy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (ax + t * vx), py - (ay + t * vy));
}

function roundedBoxDistance(x, y, halfW, halfH, radius) {
  const qx = Math.abs(x) - (halfW - radius);
  const qy = Math.abs(y) - (halfH - radius);
  return (
    Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) +
    Math.min(Math.max(qx, qy), 0) -
    radius
  );
}

const RING_RADIUS = 0.70;
const RING_HALF_THICKNESS = 0.155;
const GLYPH_EXTENT = RING_RADIUS + RING_HALF_THICKNESS;

/** A clock face: ring plus an hour and a minute hand. */
function clockField(x, y) {
  const ring = Math.abs(Math.hypot(x, y) - RING_RADIUS) - RING_HALF_THICKNESS;
  const hourHand = segmentDistance(x, y, 0, 0.02, 0, -0.34) - 0.115;
  const minuteHand = segmentDistance(x, y, 0, 0.02, 0.40, 0.12) - 0.10;
  return Math.min(ring, hourHand, minuteHand);
}

/** Same ring, but with a pause glyph inside — used while reminders are off. */
function pausedField(x, y) {
  const ring = Math.abs(Math.hypot(x, y) - RING_RADIUS) - RING_HALF_THICKNESS;
  const left = roundedBoxDistance(x + 0.19, y, 0.105, 0.27, 0.05);
  const right = roundedBoxDistance(x - 0.19, y, 0.105, 0.27, 0.05);
  return Math.min(ring, left, right);
}

/* ------------------------------------------------------------ rasteriser */

const SUPERSAMPLE = 4;

/**
 * @param {number} size          output size in pixels
 * @param {function} field       signed distance field in glyph space
 * @param {object} options       { fill, background }
 */
function render(size, field, options) {
  const { fill, background = null, padding = 0.06 } = options;
  const rgba = Buffer.alloc(size * size * 4);
  // Map pixel space to glyph space so the glyph spans (1 - padding) of the icon.
  const scale = GLYPH_EXTENT / (1 - padding);
  const step = 1 / (size * SUPERSAMPLE);

  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let glyphHits = 0;
      let backdropHits = 0;
      let backdropY = 0;

      for (let sy = 0; sy < SUPERSAMPLE; sy++) {
        for (let sx = 0; sx < SUPERSAMPLE; sx++) {
          const u = ((px + (sx + 0.5) / SUPERSAMPLE) / size) * 2 - 1;
          const v = ((py + (sy + 0.5) / SUPERSAMPLE) / size) * 2 - 1;
          if (field(u * scale, v * scale) < 0) glyphHits++;
          if (background) {
            const d = roundedBoxDistance(u, v, 0.99, 0.99, background.radius);
            if (d < 0) {
              backdropHits++;
              backdropY += (v + 1) / 2;
            }
          }
        }
      }

      const samples = SUPERSAMPLE * SUPERSAMPLE;
      const offset = (py * size + px) * 4;
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;

      if (background && backdropHits > 0) {
        const t = backdropY / backdropHits;
        r = background.from[0] + (background.to[0] - background.from[0]) * t;
        g = background.from[1] + (background.to[1] - background.from[1]) * t;
        b = background.from[2] + (background.to[2] - background.from[2]) * t;
        a = backdropHits / samples;
      }

      const glyphAlpha = glyphHits / samples;
      if (glyphAlpha > 0) {
        const outA = glyphAlpha + a * (1 - glyphAlpha);
        r = (fill[0] * glyphAlpha + r * a * (1 - glyphAlpha)) / outA;
        g = (fill[1] * glyphAlpha + g * a * (1 - glyphAlpha)) / outA;
        b = (fill[2] * glyphAlpha + b * a * (1 - glyphAlpha)) / outA;
        a = outA;
      }

      rgba[offset] = Math.round(r);
      rgba[offset + 1] = Math.round(g);
      rgba[offset + 2] = Math.round(b);
      rgba[offset + 3] = Math.round(a * 255);
      step; // keep the linter honest about unused locals
    }
  }
  return rgba;
}

/* ----------------------------------------------------------------- write */

const OUT = path.join(__dirname, '..', 'assets');
const BLACK = [0, 0, 0];
const WHITE = [255, 255, 255];
const BACKDROP = { from: [34, 211, 238], to: [79, 70, 229], radius: 0.42 };

function write(name, size, field, options) {
  const file = path.join(OUT, name);
  fs.writeFileSync(file, encodePng(size, render(size, field, options)));
  console.log(`  ${name}  ${size}x${size}`);
}

fs.mkdirSync(OUT, { recursive: true });
console.log('generating icons ->', OUT);

// macOS menu bar: template images are tinted by the system, so pure black + alpha.
write('trayTemplate.png', 16, clockField, { fill: BLACK, padding: 0.10 });
write('trayTemplate@2x.png', 32, clockField, { fill: BLACK, padding: 0.10 });
write('tray-pausedTemplate.png', 16, pausedField, { fill: BLACK, padding: 0.10 });
write('tray-pausedTemplate@2x.png', 32, pausedField, { fill: BLACK, padding: 0.10 });

// Windows / Linux trays are not template-aware, so ship the coloured badge.
write('tray.png', 32, clockField, { fill: WHITE, background: BACKDROP, padding: 0.28 });
write('tray@2x.png', 64, clockField, { fill: WHITE, background: BACKDROP, padding: 0.28 });
write('tray-paused.png', 32, pausedField, { fill: WHITE, background: BACKDROP, padding: 0.28 });
write('tray-paused@2x.png', 64, pausedField, { fill: WHITE, background: BACKDROP, padding: 0.28 });

// Application icon, used by electron-builder to derive .icns / .ico.
write('icon.png', 1024, clockField, { fill: WHITE, background: BACKDROP, padding: 0.30 });
write('icon-256.png', 256, clockField, { fill: WHITE, background: BACKDROP, padding: 0.30 });

console.log('done');
