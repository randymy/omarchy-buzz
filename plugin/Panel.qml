import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import qs.Commons

Item {
  id: root
  property var shell: null
  property var manifest: null
  property var service: null
  property bool opened: false
  onOpenedChanged: if (service) service.panelOpen = opened
  Component.onDestruction: if (service) service.panelOpen = false

  function open(payloadJson) {
    // Navigation payloads and external data are not accepted in the preview.
    var focused = Hyprland.focusedMonitor
    var target = focused ? Quickshell.screens.find(function(screen) { return screen.name === focused.name }) : null
    if (target) window.screen = target
    opened = true
    Qt.callLater(function() { content.forceActiveFocus() })
  }

  function close() { opened = false }

  function dismiss() {
    // Clear the host's open-panel set so its lazy loader can release this UI.
    // close() stays separate because the host invokes it from hide().
    if (shell && manifest) shell.hide(manifest.id)
    else close()
  }

  PanelWindow {
    id: window
    visible: root.opened
    implicitWidth: Math.min(Style.space(820), (screen ? screen.width : 900) - Style.space(32))
    implicitHeight: Math.min(Style.space(570), (screen ? screen.height : 700) - Style.space(64))
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.namespace: "omarchy-buzz"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: root.opened ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    PanelContent {
      id: content
      anchors.fill: parent
      service: root.service
      onCloseRequested: root.dismiss()
    }
  }
}
