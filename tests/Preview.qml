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
      recipientPickerExpanded: !!Quickshell.env("BUZZ_PREVIEW_RECIPIENTS")
      anchors.fill: parent
      service: Quickshell.env("BUZZ_PREVIEW_CATALOG") || Quickshell.env("BUZZ_PREVIEW_HISTORY") || Quickshell.env("BUZZ_PREVIEW_SEND") ? protocolService : sampleService
    }
  }

  // Setup states with and without the helper's `setup_assist` capability.
  Buzz.Service { id: setupService; autoConnect: false }
  FloatingWindow {
    visible: true
    implicitWidth: 820
    implicitHeight: 570
    Buzz.PanelContent { id: setupView; anchors.fill: parent; service: setupService }
  }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(name) { return findNamed(setupView, name, []).filter(function(entry) { return entry.visible }).length }
  function setupFrame(kind, generation, capabilities, connection, category, relay, identity) {
    return JSON.stringify({version: 1, type: kind, instanceId: "setup-fixture", generation: generation,
      capabilities: capabilities, status: {generation: generation, connection: connection, category: category,
        identity: identity, relay: relay}})
  }
  function checkSetupStates() {
    var assist = ["connection_status", "setup_assist"]
    var key = "e".repeat(64)
    setupService.beginSession()
    if (!setupService.acceptFrame(setupFrame("hello", 1, ["connection_status"], "unconfigured", null, null, null))
        || shown("buzzSetupRelay") || shown("buzzSetupRelayUrl") || shown("buzzCreateIdentity") || shown("buzzExistingIdentityNote")
        || setupService.setupInstructions.indexOf("omarchy-buzz setup relay") === -1)
      throw new Error("Helper without setup_assist lost terminal setup or showed setup controls")
    setupService.beginSession()
    if (!setupService.acceptFrame(setupFrame("hello", 1, assist, "unconfigured", null, null, null))
        || shown("buzzSetupRelay") !== 1 || shown("buzzSetupRelayUrl") !== 1 || shown("buzzCreateIdentity") || shown("buzzExistingIdentityNote")
        || setupService.setupInstructions.indexOf("Use this relay") === -1)
      throw new Error("Setup assist relay entry incorrect")
    if (!setupService.acceptFrame(setupFrame("status", 2, assist, "unconfigured", null, "wss://fixture.example/", null))
        || shown("buzzCreateIdentity") !== 1 || shown("buzzExistingIdentityNote") !== 1 || shown("buzzNewIdentityNote") !== 1
        || shown("buzzPublicKey") || findNamed(setupView, "buzzSetupRelayUrl", [])[0].text !== "wss://fixture.example/")
      throw new Error("Identity choices not offered for a configured relay")
    if (!setupService.acceptFrame(setupFrame("status", 3, assist, "unconfigured", "identity_missing", "wss://fixture.example/", key))
        || shown("buzzCreateIdentity") || shown("buzzExistingIdentityNote") !== 1 || shown("buzzPublicKey") !== 1
        || findNamed(setupView, "buzzPublicKey", [])[0].text !== "Public key " + key.slice(0, 12) + "…")
      throw new Error("Missing secret offered a replacement identity or hid the public key")
    if (!setupService.acceptFrame(setupFrame("status", 3, assist, "disconnected", "relay_unavailable", "wss://fixture.example/", key))
        || shown("buzzSetupRelay") !== 1 || shown("buzzCreateIdentity") || shown("buzzExistingIdentityNote"))
      throw new Error("Disconnected setup controls incorrect")
    // A rejected sign-in explained by the relay's clock (`clock_skew`).
    function skewFrame(connection, category, skew) {
      var f = JSON.parse(setupFrame("status", 3, assist, connection, category, "wss://fixture.example/", key))
      f.status.clockSkewSeconds = skew
      return JSON.stringify(f)
    }
    if (!setupService.acceptFrame(skewFrame("connecting", "clock_skew", 4380))
        || setupService.clockSkewSeconds !== 4380
        || setupService.statusLabel !== "Clock is off by 73 min behind"
        || setupService.barLabel !== "Clock" || setupService.barSymbol !== "!"
        || setupService.setupInstructions.indexOf("clock disagrees with the relay") === -1
        || setupService.setupInstructions.indexOf("sudo systemctl restart systemd-timesyncd") === -1
        || setupService.setupInstructions.indexOf("timedatectl") === -1
        || setupService.setupInstructions.indexOf("suspended") === -1
        || setupService.setupInstructions.indexOf("Retry") === -1
        || findNamed(setupView, "buzzSetupInstructions", [])[0].text !== setupService.setupInstructions
        || shown("buzzSetupInstructions") !== 1)
      throw new Error("Clock skew state incorrect")
    if (!setupService.acceptFrame(skewFrame("disconnected", "clock_skew", -4410))
        || setupService.statusLabel !== "Clock is off by 74 min ahead" || setupService.barLabel !== "Clock")
      throw new Error("Clock ahead of the relay mislabelled")
    if (!setupService.acceptFrame(skewFrame("disconnected", "clock_skew", 3 * 3600))
        || setupService.statusLabel !== "Clock is off by 3 h behind")
      throw new Error("Large clock skew mislabelled")
    if (!setupService.acceptFrame(skewFrame("disconnected", "clock_skew", null)) || setupService.statusLabel !== "Clock is off")
      throw new Error("Unknown clock skew mislabelled")
    // A small offset stays informational: an ordinary rejection keeps its text.
    if (!setupService.acceptFrame(skewFrame("disconnected", "auth_rejected", 30))
        || setupService.statusLabel !== "Disconnected" || setupService.barLabel !== "Offline"
        || setupService.setupInstructions.indexOf("clock") !== -1)
      throw new Error("Small skew changed the rejected state")
    var badSkews = [1.5, "60", 315576001, -315576001, true, {}]
    for (var b = 0; b < badSkews.length; b++) {
      setupService.beginSession()
      setupService.acceptFrame(setupFrame("hello", 1, assist, "unconfigured", null, null, null))
      if (setupService.acceptFrame(skewFrame("disconnected", "clock_skew", badSkews[b])))
        throw new Error("Invalid clockSkewSeconds accepted: " + JSON.stringify(badSkews[b]))
    }
    setupService.beginSession()
    if (!setupService.acceptFrame(setupFrame("hello", 1, assist, "unconfigured", null, null, null))
        || setupService.clockSkewSeconds !== null)
      throw new Error("A helper without clockSkewSeconds was refused")
    if (!setupService.acceptFrame(setupFrame("status", 3, assist, "unavailable", "identity_access_pending", "wss://fixture.example/", key))
        || shown("buzzSetupRelay") || setupService.setupRelay("wss://other.example") || setupService.createIdentity())
      throw new Error("Setup offered while the secret store is unlocking")
    if (!setupService.acceptFrame(setupFrame("status", 3, assist, "authenticated", null, "wss://fixture.example/", key))
        || shown("buzzSetupRelay") || shown("buzzCreateIdentity") || shown("buzzCreatedIdentity")
        || setupService.setupRelay("wss://other.example") || setupService.createIdentity())
      throw new Error("Setup offered while authenticated")
    // A setup refusal for another request never ends the session.
    if (!setupService.acceptFrame(JSON.stringify({version: 1, type: "error", id: "00000000-0000-4000-8000-000000000009",
          category: "setup_not_allowed", instanceId: "setup-fixture"})) || setupService.sessionFailed)
      throw new Error("Setup error category ended the session")
    if (setupService.acceptFrame(setupFrame("status", 3, assist.concat(["setup_assist"]), "authenticated", null, null, null)))
      throw new Error("Duplicate capability accepted")
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
        checkSetupStates()
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
        var roomA = {id: "00000000-0000-4000-8000-000000000001", name: "General", description: "Synthetic catalog", kind: "stream", participants: [], hidden: false}
        var roomB = {id: "00000000-0000-4000-8000-000000000002", name: "Development", description: "Synthetic catalog", kind: "stream", participants: [], hidden: false}
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
        if (protocolService.acceptFrame(catalogFrame("hello", 1, {state:"ready",rooms:[{id:"not-uuid",name:"bad",description:"",kind:"stream",participants:[],hidden:false}],category:null})))
          throw new Error("Malformed room accepted")
        protocolService.beginSession()
        if (protocolService.acceptFrame(catalogFrame("hello", 1, {state:"partial",rooms:Array(21).fill(roomA),category:null})))
          throw new Error("Oversized catalog accepted")
        var dmA = {id: "00000000-0000-4000-8000-00000000000d", name: "Ada, Bob", description: "", kind: "dm", participants: ["a".repeat(64), "b".repeat(64), "c".repeat(64)], hidden: false}
        var dmHidden = Object.assign({}, dmA, {id: "00000000-0000-4000-8000-00000000000e", name: "Hidden", participants: ["a".repeat(64), "d".repeat(64)], hidden: true})
        protocolService.beginSession()
        if (!protocolService.acceptFrame(catalogFrame("hello", 1, {state:"partial",rooms:[dmA, roomA, dmHidden],category:"room_catalog_partial"}))
            || protocolService.rooms.length !== 3 || protocolService.streamRooms.length !== 1 || protocolService.streamRooms[0].id !== roomA.id
            || protocolService.dmRooms.length !== 1 || protocolService.dmRooms[0].id !== dmA.id
            || protocolService.dmRooms[0].participants.length !== 3 || protocolService.selectedRoomId !== roomA.id
            || protocolService.roomTitle(protocolService.dmRooms[0]) !== "Ada, Bob" || protocolService.roomTitle(roomA) !== "# General")
          throw new Error("DM rooms misclassified, hidden DM listed or first visible room not selected")
        protocolService.selectRoom(dmA.id)
        if (protocolService.selectedRoomId !== dmA.id || protocolService.roomTitle(protocolService.selectedRoom) !== "Ada, Bob")
          throw new Error("DM selection or title incorrect")
        var badDms = [
          Object.assign({}, dmA, {participants: ["a".repeat(64), "A".repeat(64)]}),
          Object.assign({}, dmA, {participants: ["a".repeat(64), "a".repeat(64)]}),
          Object.assign({}, dmA, {participants: ["a".repeat(64)]}),
          Object.assign({}, dmA, {participants: "a".repeat(64)}),
          Object.assign({}, dmA, {participants: Array(10).fill(0).map(function(_, i) { return String(i).repeat(64) })}),
          Object.assign({}, dmA, {hidden: "false"}),
          Object.assign({}, dmA, {kind: "forum"}),
          Object.assign({}, roomA, {participants: ["a".repeat(64), "b".repeat(64)]}),
          Object.assign({}, roomA, {hidden: true}),
          {id: roomA.id, name: roomA.name, description: roomA.description},
          Object.assign({}, roomA, {extra: 1})
        ]
        for (var d = 0; d < badDms.length; d++) {
          protocolService.beginSession()
          if (protocolService.acceptFrame(catalogFrame("hello", 1, {state:"partial",rooms:[badDms[d]],category:null})))
            throw new Error("Malformed room kind fields accepted: case " + d)
        }
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
        // The periodic joined-room check keeps the same-scope conversation on screen.
        if (!protocolService.acceptFrame(JSON.stringify(loadingCatalog))
            || protocolService.selectedRoomId !== roomB.id || protocolService.messages.length !== 1
            || protocolService.rooms.length !== 2)
          throw new Error("Catalog refresh lost selection or blanked the conversation")
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
            || protocolService.messages.length !== 1 || protocolService.pendingHistoryRequestId !== "")
          throw new Error("Queue rejection killed connection or blanked the displayed snapshot")
        protocolService.selectRoom(roomB.id)
        protocolService.pendingHistoryRequestId = "ui-2"
        if (!protocolService.acceptFrame(JSON.stringify({version:1,type:"error",category:"request_busy",id:"ui-2",instanceId:"catalog-fixture"}))
            || protocolService.sessionFailed || protocolService.messages.length !== 0
            || protocolService.historyLabel.indexOf("busy") === -1)
          throw new Error("Queue rejection of a first load was not reported")
        function deliveryFrame(delivery, gen) {
          var value = JSON.parse(historyFrame("status", gen || 1, roomA.id, [historyRow]))
          value.capabilities.push("message_send")
          value.status.delivery = delivery
          return JSON.stringify(value)
        }
        var idleDelivery = {requestId:null,roomId:null,eventId:null,state:"idle",category:null}
        protocolService.beginSession()
        var sendHello = JSON.parse(deliveryFrame(idleDelivery))
        sendHello.type = "hello"
        if (!protocolService.acceptFrame(JSON.stringify(sendHello))) throw new Error("Send capability rejected")
        protocolService.updateDraft("Synthetic draft")
        var prepared = protocolService.prepareSubmission()
        if (!prepared || prepared.text !== "Synthetic draft" || prepared.mentions.length !== 0
            || prepared.generation !== protocolService.generation || prepared.instanceId !== protocolService.instanceId)
          throw new Error("Send request missing scope or content fences")
        if (protocolService.prepareSubmission() !== null) throw new Error("Double-click submitted twice")
        var accepted = {requestId:prepared.id,roomId:roomA.id,eventId:"e".repeat(64),state:"acknowledged",category:null}
        var wrongId = Object.assign({}, accepted, {requestId:"00000000-0000-4000-8000-000000000099"})
        protocolService.acceptFrame(deliveryFrame(wrongId))
        if (protocolService.deliveryState !== "sending" || protocolService.draftText !== "Synthetic draft")
          throw new Error("Wrong delivery ID cleared draft")
        protocolService.acceptFrame(deliveryFrame(accepted))
        if (protocolService.deliveryState !== "acknowledged" || protocolService.draftText !== "")
          throw new Error("Matching acknowledgment did not clear draft")
        protocolService.updateDraft("Keep on loss")
        var lost = protocolService.prepareSubmission()
        protocolService.losePendingDelivery()
        if (protocolService.deliveryState !== "unknown" || protocolService.draftText !== "Keep on loss"
            || protocolService.prepareSubmission() !== null) throw new Error("Lost send retried or lost draft")
        protocolService.acceptFrame(deliveryFrame(Object.assign({}, accepted, {requestId:lost.id})))
        if (protocolService.deliveryState !== "unknown" || protocolService.draftText !== "Keep on loss")
          throw new Error("Late frame silently resolved unknown submission")
        protocolService.newDraft()
        protocolService.updateDraft("Rejected draft")
        var rejected = protocolService.prepareSubmission()
        protocolService.acceptFrame(deliveryFrame({requestId:rejected.id,roomId:roomA.id,eventId:null,state:"rejected",category:"send_rejected"}))
        if (protocolService.draftText !== "Rejected draft" || protocolService.deliveryState !== "rejected")
          throw new Error("Rejected send lost draft")
        if (protocolService.prepareSubmission() !== null) throw new Error("Rejected receipt silently retried")
        protocolService.newDraft(true)
        var repeated = protocolService.prepareSubmission()
        if (repeated.id === rejected.id || repeated.text !== "Rejected draft") throw new Error("Explicit retry did not preserve draft with fresh request ID")
        protocolService.acceptFrame(deliveryFrame({requestId:repeated.id,roomId:roomA.id,eventId:null,state:"failed",category:"send_request_reused"}))
        if (protocolService.prepareSubmission() !== null) throw new Error("Reused request ID silently retried")
        protocolService.newDraft(true)
        var fresh = protocolService.prepareSubmission()
        if (fresh.id === repeated.id) throw new Error("Explicit reused-ID reset retained old ID")
        protocolService.acceptFrame(deliveryFrame(idleDelivery, 2))
        if (protocolService.draftText !== "" || protocolService.deliveryState !== "unknown")
          throw new Error("Scope change retained private draft or claimed delivery")
        // Correlated actor rejection must not tear down the helper connection,
        // clear a draft, or let a later original receipt acknowledge changed intent.
        ;["send_busy", "send_request_reused", "send_invalid", "send_unavailable", "send_access_denied", "send_ledger_unavailable", "delivery_unknown"].forEach(function(category) {
          protocolService.beginSession()
          if (!protocolService.acceptFrame(JSON.stringify(sendHello))) throw new Error("Correlated error setup failed")
          protocolService.newDraft()
          protocolService.updateDraft("Preserve correlated rejection")
          var submitted = protocolService.prepareSubmission()
          var error = {version:1,type:"error",category:category,id:submitted.id,instanceId:protocolService.instanceId}
          var unrelated = Object.assign({}, error, {id:"00000000-0000-4000-8000-000000000099"})
          protocolService.acceptFrame(JSON.stringify(unrelated))
          if (protocolService.deliveryState !== "sending") throw new Error("Unrelated rejection stopped active send")
          if (!protocolService.acceptFrame(JSON.stringify(error)) || protocolService.sessionFailed
              || protocolService.deliveryCategory !== category
              || protocolService.deliveryState !== (category === "delivery_unknown" ? "unknown" : "failed")
              || protocolService.draftText !== "Preserve correlated rejection") throw new Error("Correlated error lost connection or draft: " + category)
          protocolService.acceptFrame(deliveryFrame(Object.assign({}, accepted, {requestId:submitted.id})))
          if (protocolService.draftText !== "Preserve correlated rejection") throw new Error("Rejected intent accepted original receipt")
          if ((category === "send_request_reused" || category === "delivery_unknown") && protocolService.prepareSubmission() !== null)
            throw new Error("Ambiguous or reused request silently resubmitted")
        })
        var recipientA = {key:"a".repeat(64),name:"Same name"}
        var recipientB = {key:"b".repeat(64),name:"Same name"}
        var roster = {state:"snapshot",roomId:roomA.id,entries:[recipientA,recipientB],partial:true,category:null}
        if (!protocolService.validatedRecipients(roster)
            || protocolService.validatedRecipients(Object.assign({}, roster, {entries:[recipientA,recipientA]}))
            || protocolService.validatedRecipients(Object.assign({}, roster, {entries:[{key:recipientA.key,name:"💬".repeat(17)}]}))
            || protocolService.validatedRecipients(Object.assign({}, roster, {entries:[{key:"invalid",name:"Name"}]}))
            || protocolService.validatedRecipients(Object.assign({}, roster, {entries:[{key:recipientA.key,name:"bad\nname"}]}))
            || protocolService.validatedRecipients(Object.assign({}, roster, {entries:[{key:recipientA.key,name:"bad\u202ename"}]})))
          throw new Error("Invalid recipient profile or duplicate key accepted")
        function recipientFrame(value, gen) {
          var frame = JSON.parse(deliveryFrame(idleDelivery, gen || 3))
          frame.capabilities.push("room_recipients")
          frame.status.recipients = value
          return JSON.stringify(frame)
        }
        protocolService.newDraft()
        protocolService.acceptFrame(recipientFrame(roster))
        protocolService.toggleRecipient(recipientB.key)
        protocolService.updateDraft("Exact recipient")
        var mentioned = protocolService.prepareSubmission()
        if (!mentioned || mentioned.mentions.length !== 1 || mentioned.mentions[0] !== recipientB.key)
          throw new Error("Duplicate name was not bound to selected public key")
        protocolService.toggleRecipient(recipientA.key)
        if (protocolService.selectedRecipients.length !== 1) throw new Error("Pending picker changed intent")
        protocolService.acceptFrame(recipientFrame(Object.assign({}, roster, {roomId:roomB.id,entries:[{key:"c".repeat(64),name:"Stale room"}]})))
        if (protocolService.recipientEntries[0].key !== recipientA.key) throw new Error("Stale room recipients replaced active roster")
        protocolService.losePendingDelivery()
        protocolService.newDraft(true)
        protocolService.toggleRecipient(recipientA.key)
        var changedMentions = protocolService.prepareSubmission()
        if (changedMentions.id === mentioned.id || changedMentions.mentions.length !== 2) throw new Error("Recipient intent retained stale UUID")
        protocolService.acceptFrame(recipientFrame(roster, 4))
        if (protocolService.selectedRecipients.length !== 0 || protocolService.draftText !== "")
          throw new Error("Scope change retained recipient selection or private draft")
        protocolService.newDraft()
        protocolService.updateDraft("Keep across roster refresh")
        protocolService.toggleRecipient(recipientB.key)
        var catalogLoading = JSON.parse(recipientFrame(roster, 4))
        catalogLoading.status.catalog = {state:"loading",rooms:[],category:null}
        protocolService.acceptFrame(JSON.stringify(catalogLoading))
        // The roster stays on screen; submissions are held until the helper revalidates it.
        if (protocolService.selectedRecipients.length !== 1 || protocolService.selectedRecipients[0] !== recipientB.key
            || protocolService.recipientEntries.length !== roster.entries.length || protocolService.resyncStage !== "catalog"
            || protocolService.draftText !== "Keep across roster refresh")
          throw new Error("Catalog refresh discarded intent or hid the roster")
        protocolService.acceptFrame(recipientFrame(roster, 4))
        if (!protocolService.canSend || protocolService.selectedRecipients[0] !== recipientB.key)
          throw new Error("Validated refreshed roster did not restore preserved intent")
        protocolService.acceptFrame(recipientFrame(Object.assign({}, roster, {entries:[recipientA]}), 4))
        if (protocolService.selectedRecipients.length !== 1 || protocolService.canSend || protocolService.unavailableRecipients.length !== 1)
          throw new Error("Missing recipient was silently removed or allowed to send")
        protocolService.toggleRecipient(recipientB.key)
        if (protocolService.selectedRecipients.length !== 0 || !protocolService.canSend)
          throw new Error("Unavailable recipient could not be explicitly deselected")
        protocolService.acceptFrame(recipientFrame(roster, 4))
        protocolService.toggleRecipient(recipientA.key)
        protocolService.selectRoom(roomB.id)
        if (protocolService.selectedRecipients.length) throw new Error("Room change inherited another room's recipients")
        protocolService.selectRoom(roomA.id)
        if (protocolService.selectedRecipients.length !== 1 || protocolService.selectedRecipients[0] !== recipientA.key || protocolService.canSend)
          throw new Error("Room draft lost intent or used an unvalidated returning roster")
        protocolService.acceptFrame(recipientFrame(roster, 4))
        // Attachments: exact shapes against the configured relay, only with the capability.
        function attachmentFrame(kind, rows, extra) {
          var value = JSON.parse(historyFrame(kind, 1, roomA.id, rows))
          value.capabilities.push("attachments")
          value.status.relay = "wss://relay.example/"
          value.status.identity = "f".repeat(64)
          value.status.download = {state:"idle",eventId:null,hash:null,path:null,received:0,size:null,category:null}
          value.status.thumbnails = []
          value.status.pendingAttachments = []
          value.status.upload = {state:"idle",scope:null,name:null,category:null}
          for (var key in (extra || {})) value.status[key] = extra[key]
          return JSON.stringify(value)
        }
        var pdf = {name:"report.pdf",mime:"application/pdf",size:12345,url:"https://relay.example/media/" + "1".repeat(64) + ".pdf",
          hash:"1".repeat(64),dim:null,kind:"file"}
        var png = {name:"shot.png",mime:"image/png",size:2048,url:"https://relay.example/media/" + "2".repeat(64) + ".png",
          hash:"2".repeat(64),dim:"640x480",kind:"image"}
        var attachedRow = Object.assign({}, historyRow, {attachments:[pdf, png], attachmentsUnavailable:false})
        protocolService.beginSession()
        if (!protocolService.acceptFrame(attachmentFrame("hello", [attachedRow]))
            || !protocolService.attachmentsSupported || protocolService.messages[0].attachments.length !== 2
            || protocolService.messages[0].attachments[1].dim !== "640x480" || protocolService.formatSize(12345) !== "12.3 KB")
          throw new Error("Valid attachments rejected")
        var badAttachments = [
          [Object.assign({}, pdf, {url:"https://other.example/media/" + pdf.hash + ".pdf"})],
          [Object.assign({}, pdf, {url:"http://relay.example/media/" + pdf.hash + ".pdf"})],
          [Object.assign({}, pdf, {hash:"3".repeat(64)})],
          [Object.assign({}, pdf, {kind:"image"})],
          [Object.assign({}, png, {kind:"file"})],
          [Object.assign({}, pdf, {mime:"image/svg+xml", kind:"image"})],
          [Object.assign({}, pdf, {name:"../etc/passwd"})],
          [Object.assign({}, pdf, {name:".hidden"})],
          [Object.assign({}, pdf, {name:"a\nb"})],
          [Object.assign({}, pdf, {size:0})],
          [Object.assign({}, pdf, {size:1073741825})],
          [Object.assign({}, png, {dim:"16385x1"})],
          [Object.assign({}, pdf, {extra:true})],
          [pdf, pdf],
          [pdf, png, Object.assign({}, pdf, {hash:"4".repeat(64), url:"https://relay.example/media/" + "4".repeat(64) + ".pdf"}),
            Object.assign({}, pdf, {hash:"5".repeat(64), url:"https://relay.example/media/" + "5".repeat(64) + ".pdf"}),
            Object.assign({}, pdf, {hash:"6".repeat(64), url:"https://relay.example/media/" + "6".repeat(64) + ".pdf"})]
        ]
        for (var b = 0; b < badAttachments.length; b++) {
          protocolService.beginSession()
          if (protocolService.acceptFrame(attachmentFrame("hello", [Object.assign({}, historyRow, {attachments:badAttachments[b], attachmentsUnavailable:false})])))
            throw new Error("Malformed attachment accepted: case " + b)
        }
        var badRows = [
          Object.assign({}, historyRow, {attachments:[pdf], attachmentsUnavailable:true}),
          Object.assign({}, historyRow, {attachments:[pdf], unavailable:true, attachmentsUnavailable:false}),
          Object.assign({}, historyRow, {attachments:[]}),
          Object.assign({}, historyRow, {attachmentsUnavailable:false}),
          historyRow
        ]
        for (var r = 0; r < badRows.length; r++) {
          protocolService.beginSession()
          if (protocolService.acceptFrame(attachmentFrame("hello", [badRows[r]]))) throw new Error("Malformed attachment row accepted: case " + r)
        }
        protocolService.beginSession()
        if (protocolService.acceptFrame(historyFrame("hello", 1, roomA.id, [attachedRow])))
          throw new Error("Attachments accepted without the capability")
        // More than 48 attachments across the held rows break the frame budget.
        var many = []
        for (var m = 0; m < 13; m++) {
          var four = [0, 1, 2, 3].map(function(i) {
            var h = (m * 4 + i).toString(16).padStart(2, "0").repeat(32)
            return Object.assign({}, pdf, {hash:h, url:"https://relay.example/media/" + h + ".pdf"})
          })
          many.push(Object.assign({}, historyRow, {id:(m + 16).toString(16).repeat(32).slice(0, 64), attachments:four, attachmentsUnavailable:false}))
        }
        protocolService.beginSession()
        if (protocolService.acceptFrame(attachmentFrame("hello", many))) throw new Error("More than 48 attachments accepted")
        var goodPending = {scope:roomA.id, name:"notes.pdf", mime:"application/pdf", size:3210, url:"https://relay.example/media/" + "7".repeat(64) + ".pdf", hash:"7".repeat(64), dim:null}
        var transferCases = [
          [{download:{state:"done",eventId:"a".repeat(64),hash:pdf.hash,path:"/home/u/Downloads/report.pdf",received:12345,size:12345,category:null},
            thumbnails:[{hash:png.hash,path:"/home/u/.local/state/omarchy-buzz/thumbs/" + png.hash + ".png"}],
            pendingAttachments:[goodPending, Object.assign({}, goodPending, {scope:roomA.id + ":" + "a".repeat(64)})],
            upload:{state:"uploading",scope:roomA.id,name:"notes.pdf",category:null}}, true],
          [{download:{state:"failed",eventId:"a".repeat(64),hash:pdf.hash,path:null,received:0,size:12345,category:"attachment_mismatch"},
            upload:{state:"failed",scope:null,name:null,category:"attachment_type_refused"}}, true],
          [{download:{state:"done",eventId:"a".repeat(64),hash:pdf.hash,path:"relative/report.pdf",received:12345,size:12345,category:null}}, false],
          [{download:{state:"done",eventId:"a".repeat(64),hash:pdf.hash,path:"/home/u/../x",received:12345,size:12345,category:null}}, false],
          [{download:{state:"downloading",eventId:"a".repeat(64),hash:pdf.hash,path:null,received:12346,size:12345,category:null}}, false],
          [{download:{state:"failed",eventId:"a".repeat(64),hash:pdf.hash,path:null,received:0,size:12345,category:"secret"}}, false],
          [{download:{state:"idle",eventId:null,hash:null,path:null,received:0,size:null,category:null,extra:1}}, false],
          [{thumbnails:[{hash:png.hash,path:"/tmp/" + pdf.hash + ".png"}]}, false],
          [{thumbnails:[{hash:png.hash,path:"/tmp/" + png.hash + ".svg"}]}, false],
          [{thumbnails:Array(65).fill(0).map(function(_, i) { var h = i.toString(16).padStart(2, "0").repeat(32); return {hash:h, path:"/t/" + h + ".png"} })}, false],
          [{pendingAttachments:[Object.assign({}, goodPending, {scope:"room"})]}, false],
          [{pendingAttachments:[goodPending, goodPending]}, false],
          [{pendingAttachments:[Object.assign({}, goodPending, {url:"https://other.example/media/" + goodPending.hash + ".pdf"})]}, false],
          [{upload:{state:"uploading",scope:null,name:"notes.pdf",category:null}}, false],
          [{upload:{state:"idle",scope:roomA.id,name:null,category:null}}, false]
        ]
        for (var t = 0; t < transferCases.length; t++) {
          protocolService.beginSession()
          if (protocolService.acceptFrame(attachmentFrame("hello", [attachedRow], transferCases[t][0])) !== transferCases[t][1])
            throw new Error("Transfer view validation wrong: case " + t)
        }
        protocolService.beginSession()
        protocolService.acceptFrame(attachmentFrame("hello", [attachedRow], transferCases[1][0]))
        if (protocolService.downloadedPath("a".repeat(64), pdf.hash) !== ""
            || protocolService.downloadLabelFor("a".repeat(64), pdf.hash) !== "The file did not match what the message describes, so it was not kept.")
          throw new Error("Failed download label wrong")
        protocolService.beginSession()
        protocolService.acceptFrame(attachmentFrame("hello", [attachedRow], transferCases[0][0]))
        if (protocolService.pendingFor("").length !== 1 || protocolService.pendingFor("a".repeat(64)).length !== 1
            || protocolService.thumbnailUrl(png.hash) !== "file:///home/u/.local/state/omarchy-buzz/thumbs/" + png.hash + ".png"
            || protocolService.downloadedPath("a".repeat(64), pdf.hash) !== "/home/u/Downloads/report.pdf"
            || protocolService.uploadLabelFor("") !== "Uploading notes.pdf…" || protocolService.uploadLabelFor("a".repeat(64)) !== "")
          throw new Error("Transfer views not projected per draft")
        if (Quickshell.env("BUZZ_PREVIEW_SEND")) {
          protocolService.newDraft()
          protocolService.updateDraft("A synthetic draft. No helper is running in this screenshot.")
          if (Quickshell.env("BUZZ_PREVIEW_MISSING_RECIPIENTS"))
            protocolService.acceptFrame(recipientFrame(Object.assign({}, roster, {entries:[recipientB]}), 4))
        }
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
