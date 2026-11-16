// Draws Plates' icons and writes them to src/plates/icons/:
//
// - plates.icns, the app icon macOS shows, and plates.ico, the one Windows shows
// - plates.png, the app icon at 1024px, to look at
// - tray.png, the icon in Windows' notification area
// - menu-bar.png, the template image macOS tints for the menu bar
//
// The app icon is a stack of plates on the rack's own dark panel: four in the blue of a working
// session's marker, and one more in the orange of a session that needs you, being set down on
// top, its shadow already on the plate below.
// Run `node tools/make-icons.mjs` after changing a shape or a color here, and commit what it
// writes. It needs only Node, so it runs the same on Windows and macOS.

import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { deflateSync } from 'node:zlib'

const OUT = resolve(dirname(fileURLToPath(import.meta.url)), '../src/plates/icons')

// The rack's colors, which src/plates/src/view.rs also holds as BG, BORDER, BLUE, and HOT.
// Change both together.
const RACK = 0x131417
const RACK_EDGE = 0x2a2c31
const WORKING = 0x74aeff
const NEEDS_YOU = 0xff7a45
// The panel lightens toward its top, as the rack's own window does against a dark desktop, so a
// dark icon still shows its shape in a dark Dock.
const RACK_TOP = 0x1d1f25

const rgb = (c) => [(c >> 16) & 255, (c >> 8) & 255, c & 255]
const rad = (deg) => (deg * Math.PI) / 180

function mix(c1, c2, t) {
  const k = Math.min(Math.max(t, 0), 1)
  const [a, b] = [rgb(c1), rgb(c2)]
  return a.reduce((c, v, n) => (c << 8) | Math.round(v + (b[n] - v) * k), 0)
}

// Each shape is a signed distance, negative inside, in the design's units.
function roundRect(cx, cy, hw, hh, r) {
  return (x, y) => {
    const qx = Math.abs(x - cx) - hw + r
    const qy = Math.abs(y - cy) - hh + r
    return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r
  }
}

// A closed outline's distance, its sign from how many edges a ray from the point crosses.
// Further than `margin` outside the outline's bounds, the distance to the bounds stands in for it,
// which is cheaper, and any edge or gap drawn from the outline must stay within `margin`.
function polygon(points, margin) {
  const n = points.length
  const xs = points.map((p) => p[0])
  const ys = points.map((p) => p[1])
  const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)]
  return (px, py) => {
    const ox = Math.max(x0 - px, 0, px - x1)
    const oy = Math.max(y0 - py, 0, py - y1)
    if (ox > margin || oy > margin) return Math.hypot(ox, oy)
    let d = Infinity
    let sign = 1
    for (let i = 0, j = n - 1; i < n; j = i, i++) {
      const [ax, ay] = points[i]
      const [bx, by] = points[j]
      const ex = bx - ax
      const ey = by - ay
      const wx = px - ax
      const wy = py - ay
      const t = Math.min(Math.max((wx * ex + wy * ey) / (ex * ex + ey * ey), 0), 1)
      d = Math.min(d, (wx - ex * t) ** 2 + (wy - ey * t) ** 2)
      const c1 = py >= ay
      const c2 = py < by
      const c3 = ex * wy > ey * wx
      if ((c1 && c2 && c3) || (!c1 && !c2 && !c3)) sign = -sign
    }
    return sign * Math.sqrt(d)
  }
}

// Turns a shape drawn around the origin into one centered at (x, y) and tipped by `deg`.
function placed(x, y, deg, shape) {
  const c = Math.cos(-rad(deg))
  const s = Math.sin(-rad(deg))
  return (px, py) => {
    const dx = px - x
    const dy = py - y
    return shape(dx * c - dy * s, dx * s + dy * c)
  }
}

// Draws the layers over a transparent square of `px` pixels, where the design is `units` wide.
// A pixel's coverage of a shape comes from its distance to the edge, which smooths every edge
// over one pixel at any size.
function render(px, units, layers) {
  const out = Buffer.alloc(px * px * 4)
  const unit = units / px
  const cover = (shape, x, y) => Math.min(Math.max(0.5 - shape(x, y) / unit, 0), 1)
  for (let j = 0; j < px; j++) {
    for (let i = 0; i < px; i++) {
      const x = (i + 0.5) * unit
      const y = (j + 0.5) * unit
      let [r, g, b, a] = [0, 0, 0, 0]
      for (const layer of layers) {
        // A soft layer, such as a shadow, fades over `soft` units rather than one pixel, and
        // shows only inside its `clip`.
        let k = layer.soft
          ? Math.min(Math.max(0.5 - layer.shape(x, y) / layer.soft, 0), 1)
          : cover(layer.shape, x, y)
        for (const hole of layer.holes ?? []) k *= 1 - cover(hole, x, y)
        if (layer.clip) k *= cover(layer.clip, x, y)
        k *= layer.alpha ?? 1
        if (k === 0) continue
        const [lr, lg, lb] = rgb(typeof layer.color === 'function' ? layer.color(y) : layer.color)
        r = lr * k + r * (1 - k)
        g = lg * k + g * (1 - k)
        b = lb * k + b * (1 - k)
        a = k + a * (1 - k)
      }
      const o = (j * px + i) * 4
      // r, g, and b are premultiplied by a, and PNG wants them straight.
      out[o] = a ? Math.round(r / a) : 0
      out[o + 1] = a ? Math.round(g / a) : 0
      out[o + 2] = a ? Math.round(b / a) : 0
      out[o + 3] = Math.round(a * 255)
    }
  }
  return out
}

// An ellipse's distance, approximated, which is close enough for these plates' ellipses, none
// flatter than 1 to 3.
function ellipse(cx, cy, rx, ry) {
  return (x, y) => {
    const px = x - cx
    const py = y - cy
    const k0 = Math.hypot(px / rx, py / ry)
    const k1 = Math.hypot(px / (rx * rx), py / (ry * ry))
    return k1 === 0 ? -Math.min(rx, ry) : (k0 * (k0 - 1)) / k1
  }
}

// The union of two shapes, with the corner where they meet rounded into a curve k units wide.
function smoothUnion(a, b, k) {
  const h = Math.max(k - Math.abs(a - b), 0) / k
  return Math.min(a, b) - h * h * k * 0.25
}

// A plate seen from about 20 degrees above, in units of its radius: its top face is an ellipse
// `view` tall, with a well `well` of its width sunk `sink` in the middle, and under the face, its
// rim `rim` thick and its body tapering to a foot `foot` of its width, `depth` below the face.
const PLATE = { view: 0.34, well: 0.6, sink: 0.02, rim: 0.045, foot: 0.66, depth: 0.13, soft: 0.12 }

function plate(x, y, r, deg) {
  const p = PLATE
  const top = ellipse(0, 0, r, r * p.view)
  const rim = ellipse(0, r * p.rim, r, r * p.view)
  const foot = ellipse(0, r * p.depth, r * p.foot, r * p.view * p.foot)
  const body = (u, v) => smoothUnion(Math.min(top(u, v), rim(u, v)), foot(u, v), r * p.soft)
  return {
    top: placed(x, y, deg, top),
    well: placed(x, y, deg, ellipse(0, r * p.sink, r * p.well, r * p.view * p.well)),
    body: placed(x, y, deg, body),
  }
}

// A plate's three flat tones: its face lighter than its color, as lit from above, its well its
// color, and its rim and body darker.
function tones(color) {
  return { face: mix(color, 0xffffff, 0.22), well: color, body: mix(color, 0x000000, 0.32) }
}

// The plates in radii from the bottom plate's center: four stacked, each a rim's edge above the
// last, and the orange one coming down onto them, off to one side and tipped, as a hand sets a
// plate down.
const STACK = [0, 1, 2, 3].map((i) => ({ x: 0, y: -0.085 * i, deg: 0, color: WORKING }))
const PLACING = { x: 0.14, y: -0.085 * 3 - 0.52, deg: -10, color: NEEDS_YOU }
const PLATES = [...STACK, PLACING]

// The plates' bounds in radii, sampled from the shapes themselves.
const BOUNDS = (() => {
  const shapes = PLATES.map((p) => plate(p.x, p.y, 1, p.deg))
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity]
  for (let y = -1.6; y <= 0.6; y += 0.004) {
    for (let x = -1.3; x <= 1.4; x += 0.004) {
      if (shapes.some((s) => s.body(x, y) <= 0)) {
        x0 = Math.min(x0, x)
        x1 = Math.max(x1, x)
        y0 = Math.min(y0, y)
        y1 = Math.max(y1, y)
      }
    }
  }
  return { x0, y0, x1, y1 }
})()

// The app icon in a 1024-unit square, drawn at `px` pixels.
// macOS draws an app icon's panel 824 units wide with room around it, and Windows draws it
// nearly edge to edge, so `half` is the panel's half-width and the plates scale with it.
// The plates span 560 units of macOS' 824, and sit 2% of that above center, since the stack
// carries most of their weight and plates centered by their bounds look low.
// Each plate sits on a 7-unit gap of the panel's color, which parts it from the plate under it.
function appIcon(half, px) {
  const k = half / 412
  const span = 560 * k
  const { x0, y0, x1, y1 } = BOUNDS
  const scale = span / Math.max(x1 - x0, y1 - y0)
  const at = (p) => [
    512 + (p.x - (x0 + x1) / 2) * scale,
    512 - 0.02 * span + (p.y - (y0 + y1) / 2) * scale,
  ]
  const panel = (y) => mix(RACK_TOP, RACK, ((y - (512 - half)) / (2 * half)) * 1.4)
  const layers = [
    { shape: roundRect(512, 512, half, half, 186 * k), color: RACK_EDGE },
    { shape: roundRect(512, 512, half - 5 * k, half - 5 * k, 181 * k), color: panel },
  ]
  PLATES.forEach((p, n) => {
    const [x, y] = at(p)
    const s = plate(x, y, scale, p.deg)
    const t = tones(p.color)
    layers.push({ shape: (u, v) => s.body(u, v) - 7 * k, color: panel })
    layers.push({ shape: s.body, color: t.body })
    layers.push({ shape: s.top, color: t.face })
    layers.push({ shape: s.well, color: t.well })
    // The orange plate's shadow falls on the face of the plate it's about to land on.
    if (p === STACK[STACK.length - 1]) {
      const [px] = at(PLACING)
      const shadow = ellipse(px + 0.06 * scale, y, scale * 0.78, scale * PLATE.view * 0.72)
      layers.push({ shape: shadow, clip: s.top, color: 0x000000, alpha: 0.3, soft: scale * 0.26 })
    }
  })
  return render(px, 1024, layers)
}

const MAC = 412
const WINDOWS = 488

// A coupe plate's side profile for the menu bar, in its own frame with the rim's top at y = 0:
// the rim `t` thick at its ends, the underside curving down to a foot `foot` of the half-width
// wide, `d` below the rim, and every corner rounded by `r`.
function profile({ w, d, t, r, foot }) {
  const half = w / 2 - r
  const ease = (v) => 1 - (1 - v) * (1 - v)
  const bottom = (u) => (u <= foot ? d - r : t - r + (d - t) * ease((1 - u) / (1 - foot)))
  const us = Array.from({ length: 41 }, (_, i) => i / 40)
  // The rounding and the 0.9-unit gaps reach under 2 units past the outline.
  const outline = polygon(
    [
      [-half, r],
      [half, r],
      ...us.map((u) => [half * (1 - u), bottom(1 - u)]),
      ...us.slice(1).map((u) => [-half * u, bottom(u)]),
    ],
    3,
  )
  return (x, y) => outline(x, y) - r
}

// The menu bar's template image, in an 18-unit square for macOS' 18pt menu bar icon, drawn at 2x.
// macOS uses only its alpha, so it's black: the app icon's story seen side on, a stack and a plate
// being set down on it, since at 18pt a plate seen from above reads as a coin. The stack is two
// plates rather than the app icon's four, which at 18 pixels on a 1x screen run together.
// Each plate is cut 0.9 units clear of those above it.
function menuBar(px) {
  const plate = profile({ w: 16.5, d: 3.6, t: 1.7, r: 0.8, foot: 0.4 })
  const shapes = [
    placed(9, 13, 0, plate),
    placed(9, 10, 0, plate),
    placed(9.6, 6, -9, plate),
  ]
  const layers = shapes.map((shape, n) => ({
    shape,
    holes: shapes.slice(n + 1).map((above) => (x, y) => above(x, y) - 0.9),
    color: 0x000000,
  }))
  return render(px, 18, layers)
}

const CRC = Array.from({ length: 256 }, (_, n) => {
  let c = n
  for (let i = 0; i < 8; i++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})

function crc32(buf) {
  let c = 0xffffffff
  for (const byte of buf) c = CRC[(c ^ byte) & 255] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}

function chunk(type, data) {
  const head = Buffer.alloc(8)
  head.writeUInt32BE(data.length)
  head.write(type, 4, 'ascii')
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])))
  return Buffer.concat([head, data, crc])
}

function png(px, rgba) {
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(px, 0)
  ihdr.writeUInt32BE(px, 4)
  // 8 bits per channel, RGBA.
  ihdr.set([8, 6, 0, 0, 0], 8)
  const rows = Buffer.alloc((px * 4 + 1) * px)
  for (let y = 0; y < px; y++) rgba.copy(rows, y * (px * 4 + 1) + 1, y * px * 4, (y + 1) * px * 4)
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
  return Buffer.concat([
    signature,
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(rows, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ])
}

// An .ico is a directory of images, and Windows reads each one as a whole PNG file.
function ico(sizes) {
  const images = sizes.map((px) => png(px, appIcon(WINDOWS, px)))
  const head = Buffer.alloc(6 + 16 * images.length)
  head.writeUInt16LE(1, 2)
  head.writeUInt16LE(images.length, 4)
  let offset = head.length
  images.forEach((image, n) => {
    const e = 6 + 16 * n
    // A width or height of 256 is written as 0.
    head[e] = sizes[n] % 256
    head[e + 1] = sizes[n] % 256
    head.writeUInt16LE(1, e + 4)
    head.writeUInt16LE(32, e + 6)
    head.writeUInt32LE(image.length, e + 8)
    head.writeUInt32LE(offset, e + 12)
    offset += image.length
  })
  return Buffer.concat([head, ...images])
}

// An .icns is a list of images, each under a type naming its size in points and its scale.
// The 16px and 32px 1x images are ARGB, as Apple's iconutil writes them, because macOS 26.5
// reads a PNG under their PNG types, icp4 and icp5, as garbage (measured 2026-11-16 with
// `iconutil -c iconset`, record/areas/product/log.md).
const ICNS = [
  ['ic04', 16],
  ['ic11', 32],
  ['ic05', 32],
  ['ic12', 64],
  ['ic07', 128],
  ['ic13', 256],
  ['ic08', 256],
  ['ic14', 512],
  ['ic09', 512],
  ['ic10', 1024],
]

// ARGB is "ARGB" and then the alpha, red, green, and blue planes, each run-length coded: a byte
// n under 128 is followed by n + 1 bytes to copy, and a byte n from 128 by one byte to repeat
// n - 125 times.
function argb(px, rgba) {
  const planes = [3, 0, 1, 2].map((c) => {
    const plane = Buffer.alloc(px * px)
    for (let i = 0; i < px * px; i++) plane[i] = rgba[i * 4 + c]
    return rle(plane)
  })
  return Buffer.concat([Buffer.from('ARGB', 'ascii'), ...planes])
}

function rle(data) {
  const out = []
  let i = 0
  while (i < data.length) {
    let run = 1
    while (i + run < data.length && run < 130 && data[i + run] === data[i]) run++
    if (run >= 3) {
      out.push(run + 125, data[i])
      i += run
      continue
    }
    const start = i
    while (i < data.length && i - start < 128) {
      if (i + 2 < data.length && data[i] === data[i + 1] && data[i] === data[i + 2]) break
      i++
    }
    out.push(i - start - 1, ...data.subarray(start, i))
  }
  return Buffer.from(out)
}

function icns() {
  const drawn = new Map()
  const entries = ICNS.map(([type, px]) => {
    if (!drawn.has(px)) drawn.set(px, appIcon(MAC, px))
    const rgba = drawn.get(px)
    const image = type === 'ic04' || type === 'ic05' ? argb(px, rgba) : png(px, rgba)
    const head = Buffer.alloc(8)
    head.write(type, 0, 'ascii')
    head.writeUInt32BE(8 + image.length, 4)
    return Buffer.concat([head, image])
  })
  const head = Buffer.alloc(8)
  head.write('icns', 0, 'ascii')
  head.writeUInt32BE(8 + entries.reduce((n, e) => n + e.length, 0), 4)
  return Buffer.concat([head, ...entries])
}

mkdirSync(OUT, { recursive: true })
const write = (name, data) => {
  writeFileSync(join(OUT, name), data)
  console.log(`wrote ${join('src/plates/icons', name)} (${data.length} bytes)`)
}
write('plates.png', png(1024, appIcon(MAC, 1024)))
write('plates.icns', icns())
write('plates.ico', ico([16, 20, 24, 32, 40, 48, 64, 256]))
write('tray.png', png(32, appIcon(WINDOWS, 32)))
write('menu-bar.png', png(36, menuBar(36)))
