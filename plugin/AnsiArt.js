.pragma library
.import "Identicon.js" as Identicon
// Bounded ANSI art (.ans) for avatars. Pure functions: text in, a grid of
// {ch, fg, bg} cells out. Only foreground and background colors are kept;
// every other escape sequence is removed, so nothing in the text can move a
// cursor or change a terminal. fg is "#rrggbb", or "" for the theme
// foreground; bg is "#rrggbb", or null for none (the panel shows through).

var MAX_INPUT = 262144
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

// Applies one SGR parameter list to the current colors {fg, bg}; unknown codes are ignored.
function applySgr(params, state) {
  var fg = state.fg, bg = state.bg
  var codes = params === "" ? [""] : params.split(";")
  for (var i = 0; i < codes.length; i++) {
    var code = codes[i] === "" ? 0 : parseInt(codes[i], 10)
    if (code === 0) { fg = ""; bg = null }
    else if (code === 39) fg = ""
    else if (code === 49) bg = null
    else if (code >= 30 && code <= 37) fg = BASIC[code - 30]
    else if (code >= 90 && code <= 97) fg = BASIC[code - 90 + 8]
    else if (code >= 40 && code <= 47) bg = BASIC[code - 40]
    else if (code >= 100 && code <= 107) bg = BASIC[code - 100 + 8]
    else if (code === 38 || code === 48) {
      // Extended color: 2;r;g;b or 5;n.
      var mode = codes[i + 1], value = null
      if (mode === "2") {
        var r = byte(codes[i + 2] || ""), g = byte(codes[i + 3] || ""), b = byte(codes[i + 4] || "")
        if (r >= 0 && g >= 0 && b >= 0) value = rgb(r, g, b)
        i += 4
      } else if (mode === "5") {
        var n = byte(codes[i + 2] || "")
        if (n >= 0) value = color256(n)
        i += 2
      } else i += 1
      if (value !== null) {
        if (code === 38) fg = value
        else bg = value
      }
    }
    // 1 (bold) and every other attribute are ignored.
  }
  return {fg: fg, bg: bg}
}

// Escape sequences, in order of precedence: CSI (SGR is the one kept), OSC and
// string controls up to their terminator, any other two-character escape, a lone ESC.
// A sequence cut off by clipping is removed too.
var TOKEN = /\x1b\[([0-?]*)([ -\/]*)([@-~])|\x1b\[[0-?]*[ -\/]*$|\x1b[\]PX^_][\s\S]*?(?:\x07|\x1b\\|$)|\x1b[\s\S]|\x1b|[\uD800-\uDBFF][\uDC00-\uDFFF]|[\s\S]/g
// Control characters other than newline, C1 controls, direction overrides, BOM and lone surrogates.
var DROPPED = /^[\u0000-\u0009\u000b-\u001f\u007f-\u009f\u200e\u200f\u202a-\u202e\u2066-\u2069\u061c\ufeff\uD800-\uDFFF]$/

// Text to grid: at most 256 KiB (UTF-16 units) of input, 60 rows of 120 columns.
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
  var rows = [[]], state = {fg: "", bg: null}, match
  TOKEN.lastIndex = 0
  while ((match = TOKEN.exec(text)) !== null) {
    var token = match[0]
    if (token.charCodeAt(0) === 0x1b) {
      if (match[3] === "m" && match[2] === "" && /^[0-9;]*$/.test(match[1])) state = applySgr(match[1], state)
      continue
    }
    if (token === "\n") {
      if (rows.length >= maxRows) break
      rows.push([])
      continue
    }
    if (DROPPED.test(token)) continue
    var row = rows[rows.length - 1]
    // A blank shows only its background.
    if (row.length < maxCols) row.push({ch: token, fg: token === " " ? "" : state.fg, bg: state.bg})
  }
  // A final newline does not start another row.
  if (rows.length > 1 && rows[rows.length - 1].length === 0) rows.pop()
  if (rows.length === 1 && rows[0].length === 0) rows = []
  var cols = rows.reduce(function(widest, row) { return Math.max(widest, row.length) }, 0)
  rows.forEach(function(row) { while (row.length < cols) row.push({ch: " ", fg: "", bg: null}) })
  return {rows: rows.length, cols: cols, cells: rows}
}

function blank(cell) { return !cell || /^\s$/.test(cell.ch) }

// Downsamples to at most rows × cols by taking, for each block, the cell at its
// centre, or the non-blank cell nearest the centre when the centre is blank
// (ties go to the earlier row, then column); a block with no non-blank cell
// keeps its centre cell and so its background. Colors are kept, never averaged.
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
        chosen = best || chosen
      }
      line.push({ch: chosen.ch, fg: blank(chosen) ? "" : chosen.fg, bg: chosen.bg})
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

// The one color a thumbnail block is filled with: a character's own color
// (the theme foreground when it has none), or a blank's background. A
// character's background is not used: in art that sets one behind every
// character (a dark field behind the face), it would hide the drawing.
// "" means the block stays empty.
function blockColor(cell, ink) {
  if (!cell) return ""
  if (!blank(cell)) return cell.fg !== "" ? cell.fg : ink
  return cell.bg !== null && cell.bg !== undefined ? cell.bg : ""
}

function triple(hex) { return parseInt(hex.slice(1, 3), 16) + ";" + parseInt(hex.slice(3, 5), 16) + ";" + parseInt(hex.slice(5, 7), 16) }

// Canonical stored form of a grid: a leading reset marks it as grid art, then
// each row's characters with one SGR (reset, then truecolor foreground and
// background) wherever the colors change, and a reset before each newline.
// A blank's foreground does not matter and never forces a change.
// parse(serialize(grid)) gives the same grid back.
function serialize(grid) {
  if (!grid || grid.rows === 0 || (toArt(grid) === "" && !grid.cells.some(function(row) {
    return row.some(function(cell) { return cell.bg !== null })
  }))) return ""
  return "\x1b[0m" + grid.cells.map(function(row) {
    var fg = "", bg = null, out = ""
    row.forEach(function(cell) {
      var wantFg = blank(cell) ? fg : cell.fg
      if (wantFg !== fg || cell.bg !== bg) {
        out += "\x1b[0" + (wantFg !== "" ? ";38;2;" + triple(wantFg) : "") + (cell.bg !== null ? ";48;2;" + triple(cell.bg) : "") + "m"
        fg = wantFg
        bg = cell.bg
      }
      out += cell.ch
    })
    return out + (fg !== "" || bg !== null ? "\x1b[0m" : "")
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

// Rows and widest row of art as typed, before any clipping: escape sequences
// are not counted, control characters and a final newline are ignored.
function measure(text) {
  var lines = [0], match
  TOKEN.lastIndex = 0
  while ((match = TOKEN.exec(text)) !== null) {
    var token = match[0]
    if (token.charCodeAt(0) === 0x1b) continue
    if (token === "\n") { lines.push(0); continue }
    if (!DROPPED.test(token)) lines[lines.length - 1]++
  }
  while (lines.length > 0 && lines[lines.length - 1] === 0) lines.pop()
  return {rows: lines.length, cols: lines.reduce(function(widest, n) { return Math.max(widest, n) }, 0)}
}

// Checks art pasted or typed by hand with the limits the avatar keeps, and says
// what is wrong in plain words. {ok, art, errors, rows, cols, colored}: art is
// the stored form (storedArt) and only set when ok; errors is empty when ok.
// Plain art: at most 6 lines of 12 columns, no tabs or other control
// characters (blank lines before the art are ignored). Colored art (it holds
// escape sequences, as a .ans file does): at most 60 rows of 120 columns and
// 256 KiB; other escape sequences are removed, as when a file is loaded.
function check(text) {
  var result = {ok: false, art: "", errors: [], rows: 0, cols: 0, colored: false}
  if (typeof text !== "string") { result.errors.push("Nothing to save yet."); return result }
  if (text.length > MAX_INPUT) { result.errors.push("Too large: at most 256 KiB."); return result }
  text = text.replace(/\r\n?/g, "\n")
  var colored = isAnsi(text)
  result.colored = colored
  var size
  if (colored) {
    size = measure(text.slice(0, text.indexOf("\x1a") === -1 ? text.length : text.indexOf("\x1a")))
  } else {
    // Leading blank lines are ignored (the avatar does not show them).
    var lines = text.replace(/^(?:[^\S\n]*\n)+/, "").split("\n").map(function(line) { return line.replace(/\s+$/, "") })
    while (lines.length && lines[lines.length - 1] === "") lines.pop()
    text = lines.join("\n")
    size = {rows: lines.length, cols: Identicon.columns(text)}
    if (/[\u0000-\u0009\u000b-\u001f\u007f-\u009f‎‏‪-‮⁦-⁩؜﻿]/.test(text))
      result.errors.push("Invalid characters: tabs and other control characters are not allowed. Use spaces.")
  }
  result.rows = size.rows
  result.cols = size.cols
  var maxRows = colored ? MAX_ROWS : Identicon.MAX_LINES, maxCols = colored ? MAX_COLUMNS : Identicon.MAX_COLUMNS
  if (size.cols > maxCols) result.errors.push("Too wide: " + size.cols + " columns, at most " + maxCols + ".")
  if (size.rows > maxRows) result.errors.push("Too tall: " + size.rows + " lines, at most " + maxRows + ".")
  if (size.rows === 0 || size.cols === 0) {
    if (result.errors.length === 0) result.errors.push("Nothing to save yet.")
    return result
  }
  if (result.errors.length > 0) return result
  var art = storedArt(text)
  if (art === "") { result.errors.push("There is no visible art to save."); return result }
  result.art = art
  result.ok = true
  return result
}

// Brightness of stored grid art: 0.5–3 in steps of 0.25. New file art starts at 1.5.
var MIN_BRIGHTNESS = 0.5
var MAX_BRIGHTNESS = 3
var DEFAULT_BRIGHTNESS = 1.5
function validBrightness(value) {
  return typeof value === "number" && value >= MIN_BRIGHTNESS && value <= MAX_BRIGHTNESS && value * 4 === Math.round(value * 4)
}
function clampBrightness(value) {
  if (typeof value !== "number" || !isFinite(value)) return DEFAULT_BRIGHTNESS
  return Math.min(MAX_BRIGHTNESS, Math.max(MIN_BRIGHTNESS, Math.round(value * 4) / 4))
}

// Perceived lightness of "#rrggbb" in 0–1, on the stored (gamma-encoded)
// values: close enough to linear for leveling, and cheap.
function lightness(hex) {
  return (0.2126 * parseInt(hex.slice(1, 3), 16) + 0.7152 * parseInt(hex.slice(3, 5), 16)
    + 0.0722 * parseInt(hex.slice(5, 7), 16)) / 255
}

// A brightened copy of a grid. Auto-levels (unless levels is false) stretch
// lightness so the brightest foreground (the art's ink; backgrounds when there
// is no colored ink) reaches 0.95; then lightness is lifted by the gamma
// 1 / brightness. Foregrounds and backgrounds get the same curve. Each color
// is scaled as a whole, so its hue is kept, and each channel is clamped to
// 0–255. The theme foreground ("") and no background (null) are unchanged.
// brightness 1 with levels false returns the same colors; a missing or
// non-positive brightness returns the grid itself.
function adjust(grid, options) {
  options = options || {}
  var brightness = options.brightness
  if (!grid || typeof brightness !== "number" || !(brightness > 0)) return grid
  var brightest = 0, brightestBg = 0
  grid.cells.forEach(function(row) {
    row.forEach(function(cell) {
      if (cell.fg !== "" && !blank(cell)) brightest = Math.max(brightest, lightness(cell.fg))
      if (cell.bg !== null) brightestBg = Math.max(brightestBg, lightness(cell.bg))
    })
  })
  if (brightest === 0) brightest = brightestBg
  var gain = options.levels !== false && brightest > 0 ? 0.95 / brightest : 1
  var memo = {}
  function lift(hex) {
    if (memo.hasOwnProperty(hex)) return memo[hex]
    var value = lightness(hex)
    if (value <= 0) return (memo[hex] = hex)
    var target = Math.pow(Math.min(1, value * gain), 1 / brightness)
    var factor = target / value
    var channels = [1, 3, 5].map(function(at) {
      return Math.max(0, Math.min(255, Math.round(parseInt(hex.slice(at, at + 2), 16) * factor)))
    })
    return (memo[hex] = rgb(channels[0], channels[1], channels[2]))
  }
  return {rows: grid.rows, cols: grid.cols, cells: grid.cells.map(function(row) {
    return row.map(function(cell) {
      return {ch: cell.ch, fg: cell.fg === "" ? "" : lift(cell.fg), bg: cell.bg === null ? null : lift(cell.bg)}
    })
  })}
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
// The grid as shown: brightened when a brightness is given (0 or none: as stored).
function shownGridFor(text, brightness) {
  if (!(brightness > 0)) return gridFor(text)
  return cached("shown" + brightness, text, function() { return adjust(gridFor(text), {brightness: brightness}) })
}
function thumbnailFor(text, rows, cols, brightness) {
  return cached("thumb" + rows + "x" + cols + "@" + (brightness > 0 ? brightness : 0), text, function() {
    return thumbnail(shownGridFor(text, brightness), rows, cols)
  })
}
