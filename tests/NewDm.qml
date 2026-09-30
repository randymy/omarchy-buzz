// Synthetic stdio helper and status frames only; no relay or identity access.
import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as Buzz

ShellRoot {
  id: test
  property int stage: -1
  property int ticks: 0
  readonly property string room: "11111111-1111-4111-8111-111111111111"
  readonly property string dm: "44444444-4444-4444-8444-444444444444"
  readonly property string self: "a".repeat(64)
  readonly property string member: "c".repeat(64)
  Buzz.Service {
    id: service
    autoConnect: false
    helperExecutable: Quickshell.env("BUZZ_TEST_HELPER")
  }
  // Frame validation and a helper without `dm_open`, fed directly.
  Buzz.Service { id: offline; autoConnect: false }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 560
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  FloatingWindow {
    visible: true
    implicitWidth: 1000
    implicitHeight: 400
    Buzz.PanelContent { id: offlineView; anchors.fill: parent; service: offline }
  }
  FileView { id: record; path: Quickshell.env("BUZZ_SEND_RECORD"); blockLoading: true }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function shown(item, name) { return findNamed(item, name, []).filter(function(entry) { return entry.visible }) }
  function frame(capabilities, dmOpen) {
    var status = {generation:1,connection:"authenticated",category:null,identity:self,relay:"wss://fixture.example/",
      catalog:{state:"ready",category:null,rooms:[{id:room,name:"Fixture",description:"",kind:"stream",participants:[],hidden:false}]},
      history:{state:"snapshot",roomId:room,rows:[],hasMore:false,category:"history_completeness_unknown"},
      recipients:{state:"snapshot",roomId:room,entries:[{key:member,name:"Fixture Person"}],partial:false,category:null}}
    if (dmOpen !== undefined) status.dmOpen = dmOpen
    return JSON.stringify({version:1,type:offline.instanceId === "" ? "hello" : "status",instanceId:"offline-fixture",generation:1,
      capabilities:capabilities,status:status})
  }
  function validationCases() {
    var withDm = ["connection_status","room_catalog","room_history","room_recipients","dm_open"]
    var request = "00000000-0000-4000-8000-000000000001"
    var accepted = [
      {state:"idle",requestId:null,channelId:null,created:null,category:null},
      {state:"sending",requestId:request,channelId:null,created:null,category:null},
      {state:"acknowledged",requestId:request,channelId:dm,created:false,category:null},
      {state:"acknowledged",requestId:request,channelId:null,created:null,category:"dm_open_response_unknown"},
      {state:"rejected",requestId:request,channelId:null,created:null,category:"dm_open_rejected"},
      {state:"unknown",requestId:request,channelId:null,created:null,category:"dm_open_unknown"}
    ]
    var refused = [
      undefined,
      {state:"idle",requestId:null,channelId:null,created:null},
      {state:"idle",requestId:null,channelId:null,created:null,category:null,participants:[member]},
      {state:"idle",requestId:request,channelId:null,created:null,category:null},
      {state:"opened",requestId:request,channelId:dm,created:true,category:null},
      {state:"acknowledged",requestId:request,channelId:null,created:null,category:null},
      {state:"acknowledged",requestId:request,channelId:dm,created:null,category:null},
      {state:"acknowledged",requestId:request,channelId:"44444444-4444-4444-8444-44444444444A",created:true,category:null},
      {state:"acknowledged",requestId:"ui-1",channelId:dm,created:true,category:null},
      {state:"rejected",requestId:request,channelId:dm,created:null,category:"dm_open_rejected"},
      {state:"unknown",requestId:request,channelId:null,created:null,category:"relay said no"},
      {state:"sending",requestId:request,channelId:null,created:"yes",category:null}
    ]
    accepted.forEach(function(value, index) {
      offline.beginSession()
      if (!offline.acceptFrame(frame(withDm, value)) || !offline.dmOpenSupported)
        throw new Error("Valid DM open view " + index + " was rejected")
    })
    refused.forEach(function(value, index) {
      offline.beginSession()
      if (offline.acceptFrame(frame(withDm, value)) || !offline.sessionFailed || offline.category !== "invalid_response")
        throw new Error("Malformed DM open view " + index + " was accepted")
    })
    // Without the capability the control is hidden and nothing can be opened.
    offline.beginSession()
    if (!offline.acceptFrame(frame(["connection_status","room_catalog","room_history","room_recipients"])))
      throw new Error("Helper without dm_open was rejected")
    if (offline.dmOpenSupported || offline.dmOpenAvailable || shown(offlineView, "buzzNewDm").length
        || offline.toggleDmParticipant(member) || offline.openDm([member]))
      throw new Error("DM control offered without the dm_open capability")
    // With it, only verified members other than the viewer can be chosen.
    offline.beginSession()
    if (!offline.acceptFrame(frame(withDm, accepted[0])) || !offline.dmOpenAvailable || shown(offlineView, "buzzNewDm").length !== 1)
      throw new Error("DM control missing with the dm_open capability")
    if (offline.dmCandidates.length !== 1 || offline.dmCandidates[0].key !== member)
      throw new Error("Candidates are not the verified roster without the viewer")
    if (offline.toggleDmParticipant(self) || offline.toggleDmParticipant("d".repeat(64)) || offline.dmSelection.length)
      throw new Error("Self or unknown key was selectable")
  }
  Timer {
    interval: 50
    repeat: true
    running: true
    onTriggered: {
      try {
        test.ticks++
        if (test.ticks > 200) { console.error("New DM fixture timed out at stage " + test.stage); Qt.exit(1); return }
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.selectedRoomId === test.room && service.recipientsState === "snapshot") {
          var control = test.shown(view, "buzzNewDm")
          if (control.length !== 1) throw new Error("New message control not shown")
          if (test.shown(view, "buzzNewDmPicker").length) throw new Error("Picker open before it was asked for")
          control[0].clicked()
          if (test.shown(view, "buzzNewDmPicker").length !== 1) throw new Error("Picker did not open")
          var candidates = test.shown(view, "buzzNewDmCandidate")
          if (candidates.length !== 1 || candidates[0].key !== test.member)
            throw new Error("Picker must list the room's members without the viewer")
          if (candidates[0].text.indexOf("Synthetic person") === -1 || candidates[0].text.indexOf("cccccccc") === -1
              || candidates[0].text.indexOf("Participant") === -1)
            throw new Error("Candidate lacks name, key prefix or participant label: " + candidates[0].text)
          var start = test.shown(view, "buzzNewDmStart")[0]
          if (start.enabled) throw new Error("Start enabled with nobody chosen")
          candidates[0].clicked()
          if (JSON.stringify(service.dmSelection) !== JSON.stringify([test.member]) || !start.enabled)
            throw new Error("Choosing a member did not enable Start")
          // Self and keys from nowhere are refused even when asked directly.
          if (service.openDm([test.self]) || service.openDm(["d".repeat(64)]) || service.openDm([])
              || service.openDm([test.member, test.member]) || service.dmOpenState !== "idle")
            throw new Error("Invalid participant set was sent")
          start.clicked()
          if (service.dmOpenState !== "sending") throw new Error("Start did not send open_dm")
          if (service.openDm([test.member])) throw new Error("A second open was sent while one is pending")
          test.stage = 1
        } else if (test.stage === 1 && service.dmOpenState !== "sending") {
          if (service.dmOpenState !== "acknowledged") throw new Error("Open ended as " + service.dmOpenState)
          test.stage = 2
        } else if (test.stage === 2 && service.selectedRoomId === test.dm) {
          if (!service.dmRooms.some(function(r) { return r.id === test.dm }) || service.dmOpenTarget !== "")
            throw new Error("Opened DM not listed")
          if (test.shown(view, "buzzNewDmPicker").length || service.dmSelection.length || service.dmOpenLabel !== "")
            throw new Error("Picker or status stayed after the DM was selected")
          test.stage = 3
        } else if (test.stage === 3) {
          record.reload()
          var seen = JSON.parse(record.text()).requests.join(",")
          // Wait for the fixture process to record the opened DM's reads.
          if (seen !== "fetch_recent 11111111,fetch_recipients 11111111,open_dm,room_check_listed_dm,fetch_recent 44444444,fetch_recipients 44444444")
            return
          test.validationCases()
          console.log("PASS: New message picks a verified room member, sends one open_dm, selects the DM once listed; self, foreign keys, malformed views and a missing capability are refused")
          Qt.quit()
        }
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
