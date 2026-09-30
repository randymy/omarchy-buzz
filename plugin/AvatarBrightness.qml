import QtQuick
import QtQuick.Layouts
import qs.Ui as Ui
import qs.Commons
import "AnsiArt.js" as AnsiArt

// Brightness of grid (ANSI) avatar art: − and + in steps of 0.25 within 0.5–3.
// The owner applies the chosen value; the thumbnail follows it live.
RowLayout {
  id: root
  property string fieldName: "buzzAvatarBrightness"
  property real value: AnsiArt.DEFAULT_BRIGHTNESS
  signal chosen(real value)
  spacing: Style.space(4)
  function step(delta) {
    var next = AnsiArt.clampBrightness(value + delta)
    if (next !== value) chosen(next)
  }
  Text {
    objectName: root.fieldName
    Layout.fillWidth: true
    text: "Brightness " + root.value.toFixed(2).replace(/0$/, "")
    textFormat: Text.PlainText
    elide: Text.ElideRight
    color: Color.foreground
    opacity: 0.7
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }
  Ui.Button {
    objectName: root.fieldName + "Down"
    text: "−"
    tooltipText: "Darker"
    fontSize: Style.font.caption
    horizontalPadding: Style.space(6)
    focusable: true
    enabled: root.value > AnsiArt.MIN_BRIGHTNESS
    opacity: enabled ? 1 : 0.5
    onClicked: root.step(-0.25)
  }
  Ui.Button {
    objectName: root.fieldName + "Up"
    text: "+"
    tooltipText: "Brighter"
    fontSize: Style.font.caption
    horizontalPadding: Style.space(6)
    focusable: true
    enabled: root.value < AnsiArt.MAX_BRIGHTNESS
    opacity: enabled ? 1 : 0.5
    onClicked: root.step(0.25)
  }
}
