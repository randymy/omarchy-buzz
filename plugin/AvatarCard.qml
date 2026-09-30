import QtQuick
import QtQuick.Layouts
import qs.Commons
import "Identicon.js" as Identicon
import "AnsiArt.js" as AnsiArt

// Profile card for one author, opened from a message row's avatar. It covers
// the panel it is placed in: a click outside the card or Escape closes it.
// Colored art is drawn in full (at most 100 × 50 cells) at the largest cell
// that fits, never below 4 px per cell; other avatars are shown enlarged.
FocusScope {
  id: root
  property string key: ""
  property string name: ""
  property string art: ""
  // Grid art brightness, as on the thumbnail (0: colors as stored).
  property real brightness: 0
  property bool opened: false
  signal closed()
  visible: opened
  z: 20

  function show(authorKey, authorName, authorArt, authorBrightness) {
    key = authorKey || ""
    name = authorName || ""
    art = authorArt || ""
    brightness = authorBrightness > 0 ? authorBrightness : 0
    opened = true
    forceActiveFocus()
  }
  function close() {
    if (!opened) return
    opened = false
    closed()
  }
  Keys.onEscapePressed: function(event) { event.accepted = true; root.close() }

  readonly property bool colored: AnsiArt.isAnsi(art)
  readonly property var grid: {
    if (!colored) return null
    var full = AnsiArt.shownGridFor(art, brightness)
    return full.rows > 50 || full.cols > 100 ? AnsiArt.thumbnail(full, 50, 100) : full
  }
  readonly property real availableWidth: Math.max(0, width - Style.space(64))
  readonly property real availableHeight: Math.max(0, height - Style.space(120))
  // A terminal cell is twice as tall as wide; at most 12 px wide.
  readonly property int cellWidth: !grid || grid.cols === 0 ? 0
    : Math.max(4, Math.min(12, Math.floor(Math.min(availableWidth / grid.cols, availableHeight / (grid.rows * 2)))))

  // Click outside the card closes it; clicks on the card itself are kept.
  MouseArea {
    anchors.fill: parent
    acceptedButtons: Qt.AllButtons
    onClicked: root.close()
    onWheel: function(wheel) { wheel.accepted = true }
  }
  Rectangle {
    anchors.fill: parent
    color: Util.alpha(Color.popups.background, 0.6)
    z: -1
  }
  Rectangle {
    id: card
    objectName: "buzzAvatarCard"
    anchors.centerIn: parent
    width: Math.min(parent.width - Style.space(16), content.implicitWidth + Style.space(24))
    height: Math.min(parent.height - Style.space(16), content.implicitHeight + Style.space(24))
    color: Color.popups.background
    border.color: Color.popups.border
    border.width: Math.max(1, Style.space(1))
    radius: Style.cornerRadius
    clip: true
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
    ColumnLayout {
      id: content
      anchors.centerIn: parent
      spacing: Style.space(8)
      Canvas {
        id: portrait
        objectName: "buzzAvatarCardArt"
        visible: !!root.grid
        Layout.alignment: Qt.AlignHCenter
        implicitWidth: root.grid ? root.grid.cols * root.cellWidth : 0
        implicitHeight: root.grid ? root.grid.rows * root.cellWidth * 2 : 0
        readonly property var grid: root.opened ? root.grid : null
        readonly property color ink: Color.foreground
        onGridChanged: requestPaint()
        onInkChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        // Each cell's background, then its own character in its own color, as a terminal shows it.
        onPaint: {
          var ctx = getContext("2d")
          ctx.reset()
          if (!grid || root.cellWidth <= 0) return
          var cell = root.cellWidth
          ctx.font = Math.round(cell / 0.6) + "px \"" + Style.font.family + "\", monospace"
          ctx.textBaseline = "middle"
          ctx.textAlign = "center"
          for (var r = 0; r < grid.rows; r++) {
            for (var c = 0; c < grid.cols; c++) {
              var back = grid.cells[r][c].bg
              if (back === null) continue
              ctx.fillStyle = back
              ctx.fillRect(c * cell, r * cell * 2, cell, cell * 2)
            }
          }
          for (r = 0; r < grid.rows; r++) {
            for (c = 0; c < grid.cols; c++) {
              var item = grid.cells[r][c]
              if (/^\s$/.test(item.ch)) continue
              ctx.fillStyle = item.fg === "" ? ink : item.fg
              ctx.fillText(item.ch, c * cell + cell / 2, r * cell * 2 + cell)
            }
          }
        }
      }
      BuzzAvatar {
        visible: !root.grid
        Layout.alignment: Qt.AlignHCenter
        key: root.key
        name: root.name
        art: root.colored ? "" : root.art
        pixelSize: Style.font.body * 2
      }
      Text {
        objectName: "buzzAvatarCardName"
        Layout.alignment: Qt.AlignHCenter
        Layout.maximumWidth: Math.max(Style.space(200), portrait.implicitWidth)
        text: root.name
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.body
        font.bold: true
      }
      Text {
        objectName: "buzzAvatarCardKey"
        Layout.alignment: Qt.AlignHCenter
        text: Identicon.digits(root.key) ? "Key " + root.key.slice(0, 12) + "…" : ""
        visible: text !== ""
        textFormat: Text.PlainText
        color: Color.foreground
        opacity: 0.6
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
    }
  }
}
