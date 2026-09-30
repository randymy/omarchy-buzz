// Synthetic presentation lifecycle only: no helper, relay or credentials.
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  Buzz.Service { id: service; sampleMode: true; autoConnect: false }
  QtObject {
    id: host
    property int closes: 0
    function hide(id) {
      if (id !== "community.buzz") throw new Error("Wrong plugin close")
      closes++
      panel.close()
    }
  }
  Buzz.Panel { id: panel; service: service; shell: host; manifest: ({id:"community.buzz"}) }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shownButton(name) {
    var found = findNamed(panel.content, name, []).filter(function(item) { return item.visible })
    if (found.length !== 1) throw new Error("Expected one visible " + name + ", found " + found.length)
    return found[0]
  }
  Timer {
    interval: 300
    running: true
    onTriggered: {
      try {
        // A bare summon (Super+B) opens the normal window by default.
        panel.open('{}')
        if (!panel.opened || !panel.windowMode || !service.panelOpen) throw new Error("Default did not open as a window")
        panel.open('{"mode":"window"}')
        if (!panel.windowMode) throw new Error("Re-summon changed presentation")
        // The switch lives in Settings (account menu); the shared view keeps it open across the move.
        if (!panel.content.openSettings() || !shownButton("buzzSettingsWindow").selected) throw new Error("Settings did not show Window as current")
        shownButton("buzzSettingsOverlay").clicked()
        if (!panel.opened || panel.windowMode || !service.panelOpen || host.closes || !panel.content.settingsOpen)
          throw new Error("Switch closed shared view or left Settings")
        shownButton("buzzSettingsWindow").clicked()
        if (!panel.opened || !panel.windowMode || host.closes || !shownButton("buzzSettingsWindow").selected)
          throw new Error("Return to window closed view")
        shownButton("buzzSettingsBack").clicked()
        if (panel.content.settingsOpen) throw new Error("Back to rooms did not close Settings")
        panel.close()
        if (panel.opened || service.panelOpen || host.closes) throw new Error("Host close reentered hide")
        panel.open('{"mode":"overlay"}')
        panel.dismiss()
        if (panel.opened || service.panelOpen || host.closes !== 1) throw new Error("User close missed host lifecycle")
        console.log("PASS: shared window/overlay presentation switched from Settings and host close lifecycle")
        Qt.quit()
      } catch (e) {
        console.error(String(e))
        Qt.exit(1)
      }
    }
  }
}
