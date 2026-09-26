// Offscreen component check, not a second Omarchy shell or a Wayland test.
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  Buzz.Service { id: sampleService; sampleMode: true }
  Buzz.Service { id: protocolService; autoConnect: false }

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
        if (protocolService.rooms.length !== 0 || protocolService.messages.length !== 0)
          throw new Error("Production state exposed sample data")
        function frame(kind, instance, generation, state) {
          return JSON.stringify({version: 1, type: kind, instanceId: instance, generation: generation,
            capabilities: ["connection_status"], status: {generation: generation, connection: state,
              category: null, identity: null, relay: null}})
        }
        protocolService.beginSession()
        if (!protocolService.acceptFrame(frame("hello", "fixture-1", 2, "unconfigured")))
          throw new Error("Valid handshake rejected")
        if (!protocolService.acceptFrame(frame("status", "fixture-1", 2, "authenticated"))
            || protocolService.statusLabel.indexOf("history unavailable") === -1)
          throw new Error("Authentication misrepresented as synchronization")
        if (protocolService.acceptFrame(frame("status", "old-instance", 2, "disconnected"))
            || protocolService.acceptFrame(frame("status", "fixture-1", 1, "disconnected"))
            || protocolService.connection !== "authenticated")
          throw new Error("Stale status replaced current state")
        if (protocolService.acceptFrame(frame("status", "fixture-1", 2, "working"))
            || protocolService.connection !== "unavailable")
          throw new Error("Invented connection state accepted")
        if (protocolService.acceptFrame(frame("status", "fixture-1", 2, "authenticated")))
          throw new Error("Failed session accepted another frame")
        protocolService.beginSession()
        var pending = JSON.parse(frame("hello", "fixture-2", 1, "unavailable"))
        pending.status.category = "identity_access_pending"
        pending.status.relay = "wss://fixture.example/"
        if (!protocolService.acceptFrame(JSON.stringify(pending)) || protocolService.relay !== pending.status.relay
            || protocolService.statusLabel.indexOf("unlock") === -1 || protocolService.setupInstructions.indexOf("Unlock") === -1)
          throw new Error("Secret store wait or relay presentation incorrect")
        protocolService.beginSession()
        if (protocolService.relay !== "") throw new Error("Retry retained old relay")
        var invalidConfig = JSON.parse(frame("hello", "fixture-3", 1, "unavailable"))
        invalidConfig.status.category = "invalid_config"
        if (!protocolService.acceptFrame(JSON.stringify(invalidConfig))) throw new Error("Config reload failure rejected")
        protocolService.beginSession()
        if (protocolService.acceptFrame("not JSON") || protocolService.category !== "invalid_response")
          throw new Error("Malformed frame accepted")
        protocolService.beginSession()
        if (protocolService.acceptFrame("x".repeat(65537))) throw new Error("Oversized frame accepted")
        protocolService.beginSession()
        if (protocolService.acceptFrame(frame("status", "fixture-1", 1, "unconfigured")))
          throw new Error("Status accepted before hello")
        protocolService.beginSession()
        var incompatible = JSON.parse(frame("hello", "fixture-1", 1, "unconfigured"))
        incompatible.version = 2
        if (protocolService.acceptFrame(JSON.stringify(incompatible))) throw new Error("Wrong protocol accepted")
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
