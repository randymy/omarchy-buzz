import QtQuick
import QtQuick.Controls as Controls
import qs.Commons
import "Identicon.js" as Identicon
import "AnsiArt.js" as AnsiArt
import "PlainText.js" as PlainText

// A small monospace avatar: the key's identicon, or bounded pasted art when set.
// Presentation only; the key is a public key or a persona id, never a secret.
Text {
  id: root
  objectName: "buzzAvatar"
  property string key: ""
  property string name: ""
  // Optional art. Plain pasted art is clipped to 6 lines of 12 columns, control
  // characters removed. Art with SGR color sequences (from an .ans file) is drawn
  // as a colored 6 × 12 thumbnail inside the identicon's own footprint, so a
  // colored avatar takes exactly the space an identicon does.
  property string art: ""
  property real pixelSize: Style.font.caption
  // Grid art brightness (AnsiArt.adjust); 0 shows the colors as stored.
  property real brightness: 0
  // Message rows open a profile card; elsewhere the avatar only has a tooltip.
  property bool clickable: false
  signal activated()
  readonly property bool colored: AnsiArt.isAnsi(art)
  readonly property var thumbnail: colored ? AnsiArt.thumbnailFor(art, 6, 12, brightness) : null
  readonly property bool usesColor: colored && !!thumbnail && thumbnail.rows > 0
  // Plain art as stored, without leading blank lines: pasted art often starts
  // with an empty line, which pushed the drawing below the row it sits in.
  // (Render only: the stored form is unchanged, so saved files stay valid.)
  readonly property string shownArt: colored ? "" : Identicon.normalizeArt(art).replace(/^(?:[^\S\n]*\n)+/, "")
  readonly property bool usesArt: shownArt !== "" || usesColor
  readonly property string keyColor: Identicon.color(key)
  // Three lines of five full blocks: the same advance and height as any identicon.
  readonly property string footprint: "█████\n█████\n█████"

  text: usesColor ? footprint : usesArt ? shownArt : Identicon.glyph(key)
  textFormat: Text.PlainText
  wrapMode: Text.NoWrap
  color: usesColor ? "transparent" : usesArt || keyColor === "" ? Color.foreground : keyColor
  font.family: Style.font.family
  font.pixelSize: pixelSize
  lineHeightMode: Text.FixedHeight
  lineHeight: Math.ceil(pixelSize * 1.1)

  // Each thumbnail cell is a filled block, twice as tall as wide like a
  // terminal cell, centred in the footprint (colors: AnsiArt.blockColor).
  Canvas {
    id: thumbCanvas
    objectName: "buzzAvatarCanvas"
    anchors.fill: parent
    visible: root.usesColor
    readonly property var grid: root.usesColor ? root.thumbnail : null
    readonly property color ink: Color.foreground
    onGridChanged: requestPaint()
    onInkChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPaint: {
      var ctx = getContext("2d")
      ctx.reset()
      if (!grid || width <= 0 || height <= 0) return
      var cell = Math.min(width / grid.cols, height / (grid.rows * 2))
      var left = (width - cell * grid.cols) / 2, top = (height - cell * 2 * grid.rows) / 2
      for (var r = 0; r < grid.rows; r++) {
        for (var c = 0; c < grid.cols; c++) {
          var fill = AnsiArt.blockColor(grid.cells[r][c], ink.toString())
          if (fill === "") continue
          ctx.fillStyle = fill
          var x = Math.floor(left + c * cell), y = Math.floor(top + r * cell * 2)
          ctx.fillRect(x, y, Math.floor(left + (c + 1) * cell) - x, Math.floor(top + (r + 1) * cell * 2) - y)
        }
      }
    }
  }

  Controls.ToolTip.visible: hover.containsMouse && Controls.ToolTip.text !== ""
  Controls.ToolTip.text: {
    var prefix = Identicon.digits(key) ? key.slice(0, 8) + "…" : ""
    return PlainText.tip([name, prefix].filter(function(part) { return part !== "" }).join(" · "))
  }
  MouseArea {
    id: hover
    anchors.fill: parent
    hoverEnabled: true
    acceptedButtons: root.clickable ? Qt.LeftButton : Qt.NoButton
    cursorShape: root.clickable ? Qt.PointingHandCursor : Qt.ArrowCursor
    onClicked: root.activated()
  }
}
