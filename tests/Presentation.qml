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
  Timer {
    interval: 300
    running: true
    onTriggered: {
      try {
        panel.open('{"mode":"window"}')
        if (!panel.opened || !panel.windowMode || !service.panelOpen) throw new Error("Window did not open")
        panel.open('{}')
        if (!panel.windowMode) throw new Error("Re-summon changed presentation")
        panel.switchPresentation()
        if (!panel.opened || panel.windowMode || !service.panelOpen || host.closes) throw new Error("Switch closed shared view")
        panel.switchPresentation()
        if (!panel.opened || !panel.windowMode || host.closes) throw new Error("Return to window closed view")
        panel.close()
        if (panel.opened || service.panelOpen || host.closes) throw new Error("Host close reentered hide")
        panel.open('{"mode":"overlay"}')
        panel.dismiss()
        if (panel.opened || service.panelOpen || host.closes !== 1) throw new Error("User close missed host lifecycle")
        console.log("PASS: shared window/overlay presentation and host close lifecycle")
        Qt.quit()
      } catch (e) {
        console.error(String(e))
        Qt.exit(1)
      }
    }
  }
}
