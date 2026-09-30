.pragma library
.import "Identicon.js" as Identicon
// Bounded ANSI art (.ans) for avatars. Pure functions: text in, a grid of
// {ch, fg} cells out. Only foreground color is kept; every other escape
// sequence is removed, so nothing in the text can move a cursor or change a
// terminal. Colors are "#rrggbb", or "" for the theme foreground.

var MAX_INPUT = 65536
var MAX_ROWS = 60
var MAX_COLUMNS = 120

// Basic and bright colors (30–37, 90–97, and 256-color 0–15): mid tones that
// stay readable on light and dark panel backgrounds, not a terminal's black and white.
var BASIC = ["#6b6b6b", "#c0392b", "#2e9e44", "#b58900", "#2f6fd0", "#a347ba", "#1f9aa8", "#a8a8a8",
  "#8a8a8a", "#e5534b", "#57c267", "#d9b300", "#5b93f0", "#c77ddb", "#4cc3cf", "#d0d0d0"]

function hex2(value) { return ("0" + value.toString(16)).slice(-2) }
function rgb(r, g, b) { return "#" + hex2(r) + hex2(g) + hex2(b) }

// xterm 256-color table: 16 basic, a 6×6×6 cube, then 24 grays.
function color256(n) {
  if (n < 16) return BASIC[n]
  if (n < 232) {
    var levels = [0, 95, 135, 175, 215, 255]
    var i = n - 16
    return rgb(levels[Math.floor(i / 36)], levels[Math.floor(i / 6) % 6], levels[i % 6])
  }
  var gray = 8 + (n - 232) * 10
  return rgb(gray, gray, gray)
}

function byte(text) {
  if (!/^\d{1,3}$/.test(text)) return -1
  var value = parseInt(text, 10)
  return value <= 255 ? value : -1
}

// Applies one SGR parameter list to the current foreground; unknown codes are ignored.
function applySgr(params, fg) {
  var codes = params === "" ? [""] : params.split(";")
  for (var i = 0; i < codes.length; i++) {
    var code = codes[i] === "" ? 0 : parseInt(codes[i], 10)
    if (code === 0 || code === 39) fg = ""
    else if (code >= 30 && code <= 37) fg = BASIC[code - 30]
    else if (code >= 90 && code <= 97) fg = BASIC[code - 90 + 8]
    else if (code === 38 || code === 48) {
      // Extended color: 2;r;g;b or 5;n. Background (48) is consumed and ignored.
      var mode = codes[i + 1]
      if (mode === "2") {
        var r = byte(codes[i + 2] || ""), g = byte(codes[i + 3] || ""), b = byte(codes[i + 4] || "")
        if (code === 38 && r >= 0 && g >= 0 && b >= 0) fg = rgb(r, g, b)
        i += 4
      } else if (mode === "5") {
        var n = byte(codes[i + 2] || "")
        if (code === 38 && n >= 0) fg = color256(n)
        i += 2
      } else i += 1
    }
    // 1 (bold) and every other attribute are ignored.
  }
  return fg
}

// Escape sequences, in order of precedence: CSI (SGR is the one kept), OSC and
// string controls up to their terminator, any other two-character escape, a lone ESC.
// A sequence cut off by clipping is removed too.
var TOKEN = /\x1b\[([0-?]*)([ -\/]*)([@-~])|\x1b\[[0-?]*[ -\/]*$|\x1b[\]PX^_][\s\S]*?(?:\x07|\x1b\\|$)|\x1b[\s\S]|\x1b|[\uD800-\uDBFF][\uDC00-\uDFFF]|[\s\S]/g
// Control characters other than newline, C1 controls, direction overrides, BOM and lone surrogates.
var DROPPED = /^[\u0000-\u0009\u000b-\u001f\u007f-\u009f\u200e\u200f\u202a-\u202e\u2066-\u2069\u061c\ufeff\uD800-\uDFFF]$/

// Text to grid: at most 64 KiB (UTF-16 units) of input, 60 rows of 120 columns.
// Longer input is clipped. Rows are padded with blanks to the widest row.
function parse(text, limits) {
  limits = limits || {}
  var maxRows = Math.min(limits.rows || MAX_ROWS, MAX_ROWS)
  var maxCols = Math.min(limits.cols || MAX_COLUMNS, MAX_COLUMNS)
  var maxInput = Math.min(limits.input || MAX_INPUT, MAX_INPUT)
  if (typeof text !== "string") text = ""
  text = text.slice(0, maxInput)
  // A SAUCE record and comments follow the DOS end-of-file character.
  var eof = text.indexOf("\x1a")
  if (eof !== -1) text = text.slice(0, eof)
  var rows = [[]], fg = "", match
  TOKEN.lastIndex = 0
  while ((match = TOKEN.exec(text)) !== null) {
    var token = match[0]
    if (token.charCodeAt(0) === 0x1b) {
      if (match[3] === "m" && match[2] === "" && /^[0-9;]*$/.test(match[1])) fg = applySgr(match[1], fg)
      continue
    }
    if (token === "\n") {
      if (rows.length >= maxRows) break
      rows.push([])
      continue
    }
    if (DROPPED.test(token)) continue
    var row = rows[rows.length - 1]
    // Blanks carry no color: only the foreground is drawn.
    if (row.length < maxCols) row.push({ch: token, fg: token === " " ? "" : fg})
  }
  // A final newline does not start another row.
  if (rows.length > 1 && rows[rows.length - 1].length === 0) rows.pop()
  if (rows.length === 1 && rows[0].length === 0) rows = []
  var cols = rows.reduce(function(widest, row) { return Math.max(widest, row.length) }, 0)
  rows.forEach(function(row) { while (row.length < cols) row.push({ch: " ", fg: ""}) })
  return {rows: rows.length, cols: cols, cells: rows}
}

function blank(cell) { return !cell || /^\s$/.test(cell.ch) }

// Downsamples to at most rows × cols by taking, for each block, the cell at its
// centre, or the non-blank cell nearest the centre when the centre is blank
// (ties go to the earlier row, then column). Colors are kept, never averaged.
function thumbnail(grid, rows, cols) {
  if (!grid || grid.rows === 0 || grid.cols === 0) return {rows: 0, cols: 0, cells: []}
  var outRows = Math.max(1, Math.min(rows, grid.rows))
  var outCols = Math.max(1, Math.min(cols, grid.cols))
  var cells = []
  for (var r = 0; r < outRows; r++) {
    var line = []
    var top = Math.floor(r * grid.rows / outRows), bottom = Math.floor((r + 1) * grid.rows / outRows)
    var centreRow = (r + 0.5) * grid.rows / outRows - 0.5
    for (var c = 0; c < outCols; c++) {
      var left = Math.floor(c * grid.cols / outCols), right = Math.floor((c + 1) * grid.cols / outCols)
      var centreCol = (c + 0.5) * grid.cols / outCols - 0.5
      var chosen = grid.cells[Math.round(centreRow)][Math.round(centreCol)]
      if (blank(chosen)) {
        var best = null, bestDistance = Infinity
        for (var y = top; y < bottom; y++) {
          for (var x = left; x < right; x++) {
            var cell = grid.cells[y][x]
            if (blank(cell)) continue
            var distance = (y - centreRow) * (y - centreRow) + (x - centreCol) * (x - centreCol)
            if (distance < bestDistance) { best = cell; bestDistance = distance }
          }
        }
        chosen = best || {ch: " ", fg: ""}
      }
      line.push({ch: chosen.ch, fg: blank(chosen) ? "" : chosen.fg})
    }
    cells.push(line)
  }
  return {rows: outRows, cols: outCols, cells: cells}
}

// Plain text of a grid: trailing blanks and blank trailing lines removed.
function toArt(grid) {
  if (!grid) return ""
  var lines = grid.cells.map(function(row) { return row.map(function(cell) { return cell.ch }).join("").replace(/\s+$/, "") })
  while (lines.length && lines[lines.length - 1] === "") lines.pop()
  return lines.join("\n")
}

// Canonical stored form of a grid: a leading reset marks it as grid art, then
// each row's characters with a truecolor SGR wherever the color changes and a
// reset before each newline. parse(serialize(grid)) gives the same grid back.
function serialize(grid) {
  if (!grid || grid.rows === 0 || toArt(grid) === "") return ""
  return "\x1b[0m" + grid.cells.map(function(row) {
    var current = "", out = ""
    row.forEach(function(cell) {
      if (cell.fg !== current && !blank(cell)) {
        out += cell.fg === "" ? "\x1b[0m" : "\x1b[38;2;" + parseInt(cell.fg.slice(1, 3), 16) + ";"
          + parseInt(cell.fg.slice(3, 5), 16) + ";" + parseInt(cell.fg.slice(5, 7), 16) + "m"
        current = cell.fg
      }
      out += cell.ch
    })
    return out + (current !== "" ? "\x1b[0m" : "")
  }).join("\n")
}

// Sanitized source: what is stored and what renders. Idempotent.
function sanitize(text) { return serialize(parse(text)) }

// Grid art is recognised by its escape sequences; anything else is plain pasted art.
function isAnsi(text) { return typeof text === "string" && text.indexOf("\x1b[") !== -1 }

// Stored form for any avatar art: grid art sanitized, plain art clipped to 6 × 12.
function storedArt(text) {
  if (typeof text !== "string") return ""
  return isAnsi(text) ? sanitize(text) : Identicon.normalizeArt(text)
}

// Parsing is repeated for every avatar of the same author, so the last few
// results are kept. Grids are shared: callers must not modify them.
var cache = []
function cached(kind, text, compute) {
  for (var i = 0; i < cache.length; i++) if (cache[i].kind === kind && cache[i].text === text) return cache[i].value
  var value = compute()
  cache.unshift({kind: kind, text: text, value: value})
  if (cache.length > 16) cache.pop()
  return value
}
function gridFor(text) { return cached("grid", text, function() { return parse(text) }) }
function thumbnailFor(text, rows, cols) {
  return cached("thumb" + rows + "x" + cols, text, function() { return thumbnail(gridFor(text), rows, cols) })
}
