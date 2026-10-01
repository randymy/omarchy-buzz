import QtQuick
import QtQuick.Controls as Controls
import qs.Commons

// A small presence dot (`presence`): green online, amber away, grey offline.
// Presentation only; the state is a verified projection from the helper, and
// nothing is drawn when it is unknown.
Rectangle {
  id: root
  objectName: "buzzPresenceDot"
  property var service: null
  // "online", "away", "offline" or "" (unknown: hidden).
  property string presence: ""
  property real size: Style.space(7)
  readonly property string label: service ? service.presenceLabel(presence) : ""
  visible: !!service && service.validPresenceState(presence)
  implicitWidth: size
  implicitHeight: size
  width: size
  height: size
  radius: size / 2
  color: !!service && service.validPresenceState(presence) ? service.presenceColors[presence] : "transparent"
  border.color: Color.background
  border.width: Math.max(1, Style.space(1))
  Controls.ToolTip.visible: dotHover.containsMouse
  Controls.ToolTip.text: label
  MouseArea {
    id: dotHover
    anchors.fill: parent
    hoverEnabled: true
    acceptedButtons: Qt.NoButton
  }
}
