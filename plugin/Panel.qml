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
  // A normal window by default (Super+B and a bare summon); Settings or a
  // {"mode":"overlay"} summon switches to the overlay for the session.
  property bool windowMode: true
  // The shared view, for synthetic presentation checks.
  readonly property alias content: content
  onOpenedChanged: if (service) service.panelOpen = opened
  Component.onDestruction: if (service) service.panelOpen = false

  function open(payloadJson) {
    // Only a presentation choice is accepted; no navigation or executable data.
    if (payloadJson) {
      try {
        var payload = JSON.parse(String(payloadJson))
        if (payload && payload.mode === "window") windowMode = true
        else if (payload && payload.mode === "overlay") windowMode = false
      } catch (e) { /* malformed payload leaves the current presentation */ }
    }
    var focused = Hyprland.focusedMonitor
    var target = focused ? Quickshell.screens.find(function(screen) { return screen.name === focused.name }) : null
    if (target) overlayWindow.screen = target
    opened = true
    Qt.callLater(function() { content.forceActiveFocus() })
  }

  function close() { opened = false }

  function switchPresentation() {
    windowMode = !windowMode
    Qt.callLater(function() { content.forceActiveFocus() })
  }

  function dismiss() {
    // Clear the host's open-panel set so its lazy loader can release this UI.
    // close() stays separate because the host invokes it from hide().
    if (shell && manifest) shell.hide(manifest.id)
    else close()
  }

  PanelWindow {
    id: overlayWindow
    visible: root.opened && !root.windowMode
    implicitWidth: Math.min(Style.space(820), (screen ? screen.width : 900) - Style.space(32))
    implicitHeight: Math.min(Style.space(570), (screen ? screen.height : 700) - Style.space(64))
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.namespace: "omarchy-buzz"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
  }

  FloatingWindow {
    id: normalWindow
    visible: root.opened && root.windowMode
    title: "Buzz for Omarchy"
    implicitWidth: Style.space(900)
    implicitHeight: Style.space(680)
    minimumSize: Qt.size(Style.space(720), Style.space(500))
    color: Color.popups.background
    // Native window close must update the host's lazy panel lifecycle.
    // Hiding while switching presentation or closing from the host is ignored.
    onVisibleChanged: {
      if (!visible && root.opened && root.windowMode) root.dismiss()
    }
  }

  PanelContent {
    id: content
    parent: root.windowMode ? normalWindow.contentItem : overlayWindow.contentItem
    anchors.fill: parent
    service: root.service
    manifest: root.manifest
    presentationSwitchEnabled: true
    windowMode: root.windowMode
    onPresentationRequested: root.switchPresentation()
    onCloseRequested: root.dismiss()
  }
}
