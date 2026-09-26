// Offscreen component check, not a second Omarchy shell or a Wayland test.
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  Buzz.Service { id: sampleService }

  FloatingWindow {
    visible: true
    implicitWidth: 820
    implicitHeight: 570
    Buzz.PanelContent {
      id: content
      anchors.fill: parent
      service: sampleService
    }
  }

  QtObject {
    id: shellFacade
    property int toggleCount: 0
    function serviceFor(id) { return id === "community.buzz" ? sampleService : null }
    function toggle(id, payload) {
      if (id !== "community.buzz" || payload !== "{}") throw new Error("Invalid widget routing")
      toggleCount++
    }
  }

  QtObject {
    id: barFacade
    property var shell: shellFacade
    property bool vertical: false
    property int barSize: 32
    property color barForeground: "white"
  }

  Buzz.BarWidget {
    id: widget
    bar: barFacade
    moduleName: "community.buzz"
  }

  Timer {
    interval: 300
    running: true
    onTriggered: {
      try {
        if (widget.buzzService !== sampleService) throw new Error("Widget cannot resolve shared service")
        widget.summon()
        if (shellFacade.toggleCount !== 1) throw new Error("Widget did not route to shell")
        sampleService.selectRoom("sample-development")
        if (sampleService.messages.length !== 2 || sampleService.messages[0].roomId !== "sample-development")
          throw new Error("Room selection did not update messages")
        sampleService.selectRoom("not-a-room")
        if (sampleService.selectedRoomId !== "sample-development") throw new Error("Invalid room accepted")
        sampleService.selectRoom("sample-general")
        capture.start()
      } catch (error) {
        console.error(String(error))
        Qt.exit(1)
      }
    }
  }

  Timer {
    id: capture
    interval: 300
    onTriggered: {
      var output = Quickshell.env("BUZZ_PREVIEW_OUTPUT")
      if (!output) {
        console.log("Buzz offscreen component check passed")
        Qt.quit()
        return
      }
      content.grabToImage(function(result) {
        if (!result.saveToFile(output)) {
          console.error("Could not save preview")
          Qt.exit(1)
          return
        }
        console.log("Buzz offscreen component check passed; preview saved")
        Qt.quit()
      })
    }
  }
}
