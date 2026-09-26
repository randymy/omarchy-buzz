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
      service: Quickshell.env("BUZZ_PREVIEW_CATALOG") || Quickshell.env("BUZZ_PREVIEW_HISTORY") ? protocolService : sampleService
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
        if (protocolService.setupProvider !== "hosted") throw new Error("Hosted setup is not the default")
        var beforeProvider = [protocolService.relay, protocolService.instanceId, protocolService.generation,
          protocolService.requestSequence, protocolService.connection, protocolService.category].join("|")
        protocolService.chooseSetupProvider("custom")
        if (protocolService.setupProvider !== "custom" || protocolService.providerInstructions.indexOf("invited") === -1)
          throw new Error("Custom setup guidance incorrect")
        protocolService.chooseSetupProvider("unexpected")
        if (protocolService.setupProvider !== "custom") throw new Error("Invalid provider accepted")
        protocolService.chooseSetupProvider("hosted")
        if (beforeProvider !== [protocolService.relay, protocolService.instanceId, protocolService.generation,
          protocolService.requestSequence, protocolService.connection, protocolService.category].join("|"))
          throw new Error("Provider selection mutated connection configuration")
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
        function catalogFrame(kind, gen, catalog) {
          var value = JSON.parse(frame(kind, "catalog-fixture", gen, "authenticated"))
          value.capabilities.push("room_catalog")
          value.status.catalog = catalog
          return JSON.stringify(value)
        }
        var roomA = {id: "00000000-0000-4000-8000-000000000001", name: "General", description: "Synthetic catalog"}
        var roomB = {id: "00000000-0000-4000-8000-000000000002", name: "Development", description: "Synthetic catalog"}
        var partialCatalog = {state: "partial", rooms: [roomA, roomB], category: "room_catalog_partial"}
        protocolService.beginSession()
        if (!protocolService.acceptFrame(catalogFrame("hello", 1, partialCatalog))
            || protocolService.rooms.length !== 2 || protocolService.catalogLabel.indexOf("Partial") === -1
            || protocolService.messages.length !== 0) throw new Error("Catalog or missing history misrepresented")
        protocolService.selectRoom(roomB.id)
        if (!protocolService.acceptFrame(catalogFrame("status", 1, partialCatalog))
            || protocolService.selectedRoomId !== roomB.id) throw new Error("Same-scope selection lost")
        if (protocolService.acceptFrame(catalogFrame("status", 0, partialCatalog))) throw new Error("Invalid generation accepted")
        // Begin a new valid session after the malformed-generation failure.
        protocolService.beginSession()
        protocolService.acceptFrame(catalogFrame("hello", 2, partialCatalog))
        protocolService.selectRoom(roomB.id)
        if (protocolService.acceptFrame(catalogFrame("status", 1, {state:"ready",rooms:[roomA],category:null}))
            || protocolService.selectedRoomId !== roomB.id) throw new Error("Stale catalog replaced current selection")
        if (!protocolService.acceptFrame(catalogFrame("status", 3, partialCatalog))
            || protocolService.selectedRoomId !== roomA.id) throw new Error("New-scope selection survived")
        if (protocolService.acceptFrame(catalogFrame("status", 3, {state:"partial",rooms:[roomA,roomA],category:null}))
            || protocolService.rooms.length !== 0) throw new Error("Duplicate rooms accepted or failure retained rooms")
        protocolService.beginSession()
        if (protocolService.acceptFrame(catalogFrame("hello", 1, {state:"ready",rooms:[{id:"not-uuid",name:"bad",description:""}],category:null})))
          throw new Error("Malformed room accepted")
        protocolService.beginSession()
        if (protocolService.acceptFrame(catalogFrame("hello", 1, {state:"partial",rooms:Array(21).fill(roomA),category:null})))
          throw new Error("Oversized catalog accepted")
        protocolService.beginSession()
        protocolService.acceptFrame(catalogFrame("hello", 1, partialCatalog))
        if (!protocolService.acceptFrame(frame("status", "catalog-fixture", 1, "disconnected"))
            || protocolService.rooms.length !== 0 || protocolService.catalogState !== "unavailable")
          throw new Error("Disconnect retained catalog")
        protocolService.beginSession()
        if (!protocolService.acceptFrame(frame("hello", "old-helper", 1, "authenticated"))
            || protocolService.catalogState !== "unavailable") throw new Error("Old helper compatibility broken")
        if (Quickshell.env("BUZZ_PREVIEW_CATALOG")) {
          protocolService.beginSession()
          protocolService.acceptFrame(catalogFrame("hello", 1, partialCatalog))
          protocolService.selectRoom(roomB.id)
        }
        function historyFrame(kind, gen, room, rows) {
          var value = JSON.parse(catalogFrame(kind, gen, partialCatalog))
          value.capabilities.push("room_history")
          value.status.history = {state:"snapshot",roomId:room,rows:rows,hasMore:true,category:"history_completeness_unknown"}
          return JSON.stringify(value)
        }
        var historyRow = {id:"a".repeat(64),author:"b".repeat(64),time:1700000000,text:"Synthetic history",edited:true,truncated:false,unavailable:false}
        protocolService.beginSession()
        if (!protocolService.acceptFrame(historyFrame("hello", 1, roomA.id, [historyRow]))
            || protocolService.messages.length !== 1 || protocolService.historyLabel.indexOf("completeness") === -1)
          throw new Error("Valid history snapshot rejected or completeness overstated")
        protocolService.selectRoom(roomB.id)
        if (protocolService.messages.length !== 0) throw new Error("Room switch retained previous content")
        if (!protocolService.acceptFrame(historyFrame("status", 1, roomA.id, [historyRow]))
            || protocolService.messages.length !== 0) throw new Error("Stale room response displayed")
        if (!protocolService.acceptFrame(historyFrame("status", 1, roomB.id, [historyRow]))
            || protocolService.messages[0].author !== historyRow.author) throw new Error("Selected-room snapshot rejected")
        var loadingCatalog = JSON.parse(historyFrame("status", 1, roomB.id, [historyRow]))
        loadingCatalog.status.catalog = {state:"loading",rooms:[],category:null}
        loadingCatalog.status.history = {state:"unavailable",roomId:null,rows:[],hasMore:null,category:null}
        if (!protocolService.acceptFrame(JSON.stringify(loadingCatalog))
            || protocolService.selectedRoomId !== roomB.id || protocolService.messages.length !== 0)
          throw new Error("Catalog refresh lost selection or retained history")
        if (!protocolService.acceptFrame(historyFrame("status", 1, roomB.id, [historyRow]))
            || protocolService.selectedRoomId !== roomB.id) throw new Error("Catalog refresh failed to restore selection")
        var unavailableRow = Object.assign({}, historyRow, {unavailable:true,text:"MUST NOT DISPLAY"})
        if (!protocolService.acceptFrame(historyFrame("status", 1, roomB.id, [unavailableRow]))
            || protocolService.messages[0].text !== "") throw new Error("Unavailable content leaked")
        var badHistoryRow = Object.assign({}, historyRow, {author:"not-a-key"})
        if (protocolService.acceptFrame(historyFrame("status", 1, roomB.id, [badHistoryRow]))
            || protocolService.messages.length !== 0) throw new Error("Malformed history accepted or retained")
        protocolService.beginSession()
        var oversizedTextRow = Object.assign({}, historyRow, {text:"é".repeat(1025)})
        if (protocolService.acceptFrame(historyFrame("hello", 1, roomA.id, [oversizedTextRow])))
          throw new Error("Oversized UTF-8 history accepted")
        protocolService.beginSession()
        if (!protocolService.acceptFrame(historyFrame("hello", 1, roomA.id, [historyRow]))) throw new Error("History reset failed")
        if (!protocolService.acceptFrame(frame("status", "catalog-fixture", 1, "disconnected"))
            || protocolService.messages.length !== 0) throw new Error("Disconnected history retained")
        protocolService.beginSession()
        protocolService.acceptFrame(historyFrame("hello", 1, roomA.id, [historyRow]))
        protocolService.pendingHistoryRequestId = "ui-1"
        if (!protocolService.acceptFrame(JSON.stringify({version:1,type:"error",category:"request_busy",id:"ui-1",instanceId:"catalog-fixture"}))
            || protocolService.sessionFailed || protocolService.connection !== "authenticated"
            || protocolService.messages.length !== 0 || protocolService.historyLabel.indexOf("busy") === -1)
          throw new Error("Queue rejection killed connection or retained stale history")
        if (Quickshell.env("BUZZ_PREVIEW_HISTORY")) {
          protocolService.beginSession()
          var truncatedRow = Object.assign({}, historyRow, {id:"c".repeat(64),text:"A bounded read-only snapshot with an explicitly truncated body.",edited:false,truncated:true})
          var hiddenRow = Object.assign({}, historyRow, {id:"d".repeat(64),text:"",edited:false,unavailable:true})
          protocolService.acceptFrame(historyFrame("hello", 1, roomA.id, [historyRow,truncatedRow,hiddenRow]))
        } else if (Quickshell.env("BUZZ_PREVIEW_CATALOG")) {
          protocolService.beginSession()
          protocolService.acceptFrame(catalogFrame("hello", 1, partialCatalog))
        }
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
