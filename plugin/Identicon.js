.pragma library
// Deterministic block-character identicons and bounded pasted ASCII art.
// Pure functions: the same key gives the same glyph and color everywhere.

// A public key (64 hex) or a persona id (a UUID, for agents not yet enrolled).
// Anything else is not an identity and gets the neutral glyph.
function digits(key) {
  if (typeof key !== "string") return null
  if (/^[a-f0-9]{64}$/.test(key)) return key
  if (/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(key)) return key.replace(/-/g, "")
  return null
}

// Sixteen cells, one per hex digit; MIRROR gives each cell's left-right reflection.
var CELLS = [" ", "█", "▀", "▄", "▌", "▐", "▖", "▗", "▘", "▝", "▚", "▞", "▙", "▟", "▛", "▜"]
var MIRROR = {" ": " ", "█": "█", "▀": "▀", "▄": "▄", "▌": "▐", "▐": "▌", "▖": "▗", "▗": "▖",
  "▘": "▝", "▝": "▘", "▚": "▞", "▞": "▚", "▙": "▟", "▟": "▙", "▛": "▜", "▜": "▛"}
// The centre column must be its own mirror image.
var CENTRE = ["█", "▀", "▄", " "]
var NEUTRAL = " ▄▄▄ \n▐   ▌\n ▀▀▀ "
var ROWS = 3
var SLOTS = ROWS * 3

function neutralGlyph() { return NEUTRAL }

// Fold every digit of the key into nine slots (sum modulo 16), so the whole key
// counts; no hashing library is involved.
function slots(hex) {
  var values = []
  for (var s = 0; s < SLOTS; s++) values.push(0)
  for (var i = 0; i < hex.length; i++) values[i % SLOTS] = (values[i % SLOTS] + parseInt(hex[i], 16) * (1 + (i >> 4))) % 16
  return values
}

// Three lines of five cells, mirrored about the centre column.
function glyph(key) {
  var hex = digits(key)
  if (!hex) return NEUTRAL
  var values = slots(hex)
  var lines = []
  for (var row = 0; row < ROWS; row++) {
    var a = CELLS[values[row * 3]]
    var b = CELLS[values[row * 3 + 1]]
    var c = CENTRE[values[row * 3 + 2] % 4]
    lines.push(a + b + c + MIRROR[b] + MIRROR[a])
  }
  // A key whose pattern is all blank would be invisible; fall back to a solid centre.
  var text = lines.join("\n")
  if (!/[^\s]/.test(text)) text = "  █  \n  █  \n  █  "
  return text
}

function hue(key) {
  var hex = digits(key)
  if (!hex) return -1
  return Math.floor(parseInt(hex.slice(-4), 16) * 360 / 65536)
}

function hslToRgb(h, s, l) {
  var c = (1 - Math.abs(2 * l - 1)) * s
  var x = c * (1 - Math.abs((h / 60) % 2 - 1))
  var m = l - c / 2
  var rgb = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x]
    : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x]
  return rgb.map(function(v) { return Math.round((v + m) * 255) })
}

function luminance(rgb) {
  var lin = rgb.map(function(v) {
    var c = v / 255
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4)
  })
  return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]
}

var SATURATION = 0.55
// Relative luminance target: about 4.4:1 against white and 3.4:1 against a
// near-black background, so one color reads on both light and dark themes.
var TARGET_LUMINANCE = 0.19

// "#rrggbb", or "" for a non-key (the caller uses the theme foreground).
// Only the hue comes from the key; saturation is fixed and the HSL lightness is
// solved per hue so every key lands at the same perceived brightness.
function color(key) {
  var h = hue(key)
  if (h < 0) return ""
  var low = 0.2, high = 0.85
  for (var i = 0; i < 20; i++) {
    var mid = (low + high) / 2
    if (luminance(hslToRgb(h, SATURATION, mid)) < TARGET_LUMINANCE) low = mid
    else high = mid
  }
  var rgb = hslToRgb(h, SATURATION, (low + high) / 2)
  return "#" + rgb.map(function(v) { return ("0" + v.toString(16)).slice(-2) }).join("")
}

// Characters as the reader sees them: a surrogate pair is one column.
function characters(line) { return line.match(/[\uD800-\uDBFF][\uDC00-\uDFFF]|[\s\S]/g) || [] }

var MAX_LINES = 6
var MAX_COLUMNS = 12

// Pasted art as typed: control characters other than newline and bidirectional
// overrides removed, at most six lines of twelve characters each.
function clipArt(text) {
  if (typeof text !== "string") return ""
  var clean = text.replace(/\r\n?/g, "\n")
    .replace(/[\u0000-\u0009\u000b-\u001f\u007f-\u009f\u200e\u200f\u202a-\u202e\u2066-\u2069\u061c\ufeff]/g, "")
  var lines = clean.split("\n").slice(0, MAX_LINES)
  return lines.map(function(line) { return characters(line).slice(0, MAX_COLUMNS).join("") }).join("\n")
}

// Stored and rendered form: clipped, trailing spaces and blank trailing lines removed.
function normalizeArt(text) {
  var lines = clipArt(text).split("\n").map(function(line) { return line.replace(/\s+$/, "") })
  while (lines.length && lines[lines.length - 1] === "") lines.pop()
  return lines.join("\n")
}

function columns(text) {
  return text.split("\n").reduce(function(widest, line) { return Math.max(widest, characters(line).length) }, 0)
}
