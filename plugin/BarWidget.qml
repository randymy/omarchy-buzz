import QtQuick
import qs.Ui as Ui
import qs.Commons

Ui.BarWidget {
  id: root
  readonly property var buzzService: bar && bar.shell ? bar.shell.serviceFor(moduleName) : null
  implicitWidth: vertical ? barSize : label.implicitWidth + Style.space(20)
  implicitHeight: vertical ? Style.space(64) : barSize
  activeFocusOnTab: true

  function summon() {
    if (bar && bar.shell) bar.shell.toggle(moduleName, "{}")
  }

  Keys.onReturnPressed: summon()
  Keys.onSpacePressed: summon()
  Accessible.role: Accessible.Button
  Accessible.name: "Buzz connection"
  Accessible.onPressAction: summon()

  Text {
    id: label
    anchors.centerIn: parent
    text: root.vertical ? "B\n" + (root.buzzService ? root.buzzService.barSymbol : "!") : "Buzz · " + (root.buzzService ? root.buzzService.barLabel : "Error")
    textFormat: Text.PlainText
    horizontalAlignment: Text.AlignHCenter
    color: root.bar ? root.bar.barForeground : Color.foreground
    font.family: Style.font.family
    font.pixelSize: Style.font.body
    opacity: pointer.containsMouse || root.activeFocus ? 1 : 0.8
  }

  MouseArea {
    id: pointer
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    onClicked: root.summon()
    onEntered: {
      if (root.bar) root.bar.showTooltip(root, root.buzzService
        ? "Buzz · " + root.buzzService.statusLabel + (root.buzzService.relay ? " · " + root.buzzService.relay : "")
        : "Buzz preview · widget service unavailable on this bar")
    }
    onExited: if (root.bar) root.bar.hideTooltip(root)
  }
}
