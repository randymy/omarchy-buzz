import QtQuick
import QtQuick.Controls as Controls
import qs.Commons
import "Identicon.js" as Identicon

// A small monospace avatar: the key's identicon, or bounded pasted art when set.
// Presentation only; the key is a public key or a persona id, never a secret.
Text {
  id: root
  objectName: "buzzAvatar"
  property string key: ""
  property string name: ""
  // Optional pasted art; clipped to 6 lines of 12 columns, control characters removed.
  property string art: ""
  property real pixelSize: Style.font.caption
  readonly property string shownArt: Identicon.normalizeArt(art)
  readonly property bool usesArt: shownArt !== ""
  readonly property string keyColor: Identicon.color(key)

  text: usesArt ? shownArt : Identicon.glyph(key)
  textFormat: Text.PlainText
  wrapMode: Text.NoWrap
  color: usesArt || keyColor === "" ? Color.foreground : keyColor
  font.family: Style.font.family
  font.pixelSize: pixelSize
  lineHeightMode: Text.FixedHeight
  lineHeight: Math.ceil(pixelSize * 1.1)

  Controls.ToolTip.visible: hover.containsMouse && Controls.ToolTip.text !== ""
  Controls.ToolTip.text: {
    var prefix = Identicon.digits(key) ? key.slice(0, 8) + "…" : ""
    return [name, prefix].filter(function(part) { return part !== "" }).join(" · ")
  }
  MouseArea {
    id: hover
    anchors.fill: parent
    hoverEnabled: true
    acceptedButtons: Qt.NoButton
  }
}
