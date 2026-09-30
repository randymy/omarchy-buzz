// Pure identicon and pasted-art checks; offscreen, no helper or relay.
import QtQuick
import QtQuick.Controls as Controls
import Quickshell
import qs.Commons
import "plugin" as Buzz
import "plugin/Identicon.js" as Identicon

ShellRoot {
  id: test
  FloatingWindow {
    visible: true
    implicitWidth: 300
    implicitHeight: 200
    Column {
      Buzz.BuzzAvatar { id: keyed; key: "a".repeat(64); name: "Fixture" }
      Buzz.BuzzAvatar { id: neutral; key: "Mira"; name: "Mira" }
      Buzz.BuzzAvatar { id: pasted; key: "a".repeat(64); name: "Fixture" }
    }
  }
  function keyFrom(seed) {
    // Synthetic 64-hex keys from a small linear congruential sequence.
    var hex = "", state = seed
    for (var i = 0; i < 64; i++) { state = (state * 1103515245 + 12345) % 2147483648; hex += "0123456789abcdef"[(state >> 16) & 15] }
    return hex
  }
  function channel(hex, index) { return parseInt(hex.slice(1 + index * 2, 3 + index * 2), 16) }
  function luminance(hex) {
    return Identicon.luminance([channel(hex, 0), channel(hex, 1), channel(hex, 2)])
  }
  Timer {
    interval: 100
    running: true
    onTriggered: {
      try {
        var keys = []
        for (var s = 1; s <= 400; s++) keys.push(test.keyFrom(s))
        keys.push("a".repeat(64), "b".repeat(64), "33333333-3333-4333-8333-333333333333")
        var glyphs = {}
        keys.forEach(function(key) {
          var glyph = Identicon.glyph(key)
          if (glyph !== Identicon.glyph(key) || Identicon.color(key) !== Identicon.color(key))
            throw new Error("Identicon not deterministic for " + key)
          var lines = glyph.split("\n")
          if (lines.length !== 3) throw new Error("Glyph is not three lines: " + JSON.stringify(glyph))
          lines.forEach(function(line) {
            var cells = Identicon.characters(line)
            if (cells.length !== 5) throw new Error("Glyph line is not five cells: " + JSON.stringify(line))
            for (var c = 0; c < 5; c++) {
              if (Identicon.CELLS.indexOf(cells[c]) === -1) throw new Error("Unexpected glyph cell " + cells[c])
              if (Identicon.MIRROR[cells[c]] !== cells[4 - c]) throw new Error("Glyph not mirrored: " + JSON.stringify(line))
            }
          })
          if (!/[^\s]/.test(glyph)) throw new Error("Blank glyph for " + key)
          glyphs[glyph] = true
          var color = Identicon.color(key)
          if (!/^#[0-9a-f]{6}$/.test(color)) throw new Error("Color not #rrggbb: " + color)
          // Readable on both themes: contrast against white and against near-black.
          var lum = test.luminance(color)
          if (lum < 0.15 || lum > 0.24) throw new Error("Color " + color + " outside the readable band: " + lum)
        })
        if (Object.keys(glyphs).length < keys.length - 2) throw new Error("Distinct keys share glyphs: " + Object.keys(glyphs).length)
        if (Identicon.glyph("a".repeat(64)) === Identicon.glyph("b".repeat(64))) throw new Error("Two fixture keys share a glyph")
        var hues = {}
        keys.forEach(function(key) { hues[Identicon.hue(key)] = true })
        if (Object.keys(hues).length < 100) throw new Error("Hues do not spread across keys")
        ;[undefined, null, "", "Mira", "A".repeat(64), "a".repeat(63), "g".repeat(64), "sample-general"].forEach(function(key) {
          if (Identicon.glyph(key) !== Identicon.neutralGlyph() || Identicon.color(key) !== "")
            throw new Error("Non-key did not get the neutral glyph: " + key)
        })
        // The component: identicon colored by key, neutral glyph in the foreground color.
        if (keyed.text !== Identicon.glyph("a".repeat(64)) || !Qt.colorEqual(keyed.color, Identicon.color("a".repeat(64)))
            || keyed.font.pixelSize !== Style.font.caption || keyed.objectName !== "buzzAvatar"
            || keyed.Controls.ToolTip.text !== "Fixture · aaaaaaaa…")
          throw new Error("Keyed avatar rendered wrong")
        if (neutral.text !== Identicon.neutralGlyph() || !Qt.colorEqual(neutral.color, Color.foreground)
            || neutral.Controls.ToolTip.text !== "Mira")
          throw new Error("Neutral avatar rendered wrong")
        // Pasted art replaces the glyph, bounded to 6 x 12 with controls removed.
        pasted.art = "0123456789abcdefgh\n\tb\u0007c\u202ed\r\n3\n4\n5\n6\n7\n8"
        if (pasted.text !== "0123456789ab\nbcd\n3\n4\n5\n6" || !pasted.usesArt)
          throw new Error("Art not clipped: " + JSON.stringify(pasted.text))
        if (Identicon.clipArt("😀".repeat(14)) !== "😀".repeat(12)) throw new Error("Art clipped inside a character")
        if (Identicon.clipArt("a\n\n") !== "a\n\n" || Identicon.normalizeArt("a  \n \n\n") !== "a")
          throw new Error("Trailing lines handled wrong")
        pasted.art = " \n \n"
        if (pasted.usesArt || pasted.text !== Identicon.glyph("a".repeat(64))) throw new Error("Blank art hid the identicon")
        if (Identicon.columns("ab\nabcd") !== 4) throw new Error("Column count wrong")
        console.log("PASS: identicons are deterministic, mirrored, distinct per key, neutral for non-keys and colored within a readable band; pasted art is clipped to 6 x 12 without control characters")
        Qt.quit()
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
