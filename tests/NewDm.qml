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
  readonly property string dana: "d".repeat(64)
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
  function candidates() { return shown(view, "buzzNewDmCandidate") }
  function note() { var found = shown(view, "buzzNewDmNote"); return found.length ? found[0].text : "" }
  function type(text) { shown(view, "buzzNewDmSearch")[0].text = text }
  function frame(capabilities, dmOpen, people) {
    var status = {generation:1,connection:"authenticated",category:null,identity:self,relay:"wss://fixture.example/",
      catalog:{state:"ready",category:null,rooms:[{id:room,name:"Fixture",description:"",kind:"stream",participants:[],hidden:false}]},
      history:{state:"snapshot",roomId:room,rows:[],hasMore:false,category:"history_completeness_unknown"},
      recipients:{state:"snapshot",roomId:room,entries:[{key:member,name:"Fixture Person"}],partial:false,category:null}}
    if (dmOpen !== undefined) status.dmOpen = dmOpen
    if (people !== undefined) status.people = people
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
    // People views: only well-formed, bounded, plain ones are accepted.
    var withPeople = withDm.concat(["people_search"])
    var idle = accepted[0]
    var entry = {key: dana, name: "Dana"}
    var peopleOk = [
      {state:"unavailable",requestId:null,query:"",entries:[],category:null},
      {state:"unavailable",requestId:"ui-4",query:"x",entries:[],category:"people_timeout"},
      {state:"loading",requestId:"ui-4",query:"x",entries:[],category:null},
      {state:"snapshot",requestId:"ui-4",query:"",entries:[entry],category:null},
      {state:"snapshot",requestId:request,query:"",entries:[],category:null}
    ]
    var peopleBad = [
      undefined,
      {state:"snapshot",requestId:"ui-4",query:"",entries:[entry]},
      {state:"ready",requestId:"ui-4",query:"",entries:[],category:null},
      {state:"snapshot",requestId:null,query:"",entries:[],category:null},
      {state:"loading",requestId:null,query:"",entries:[],category:null},
      {state:"loading",requestId:"ui-4",query:"",entries:[entry],category:null},
      {state:"unavailable",requestId:"ui-4",query:"",entries:[entry],category:"people_timeout"},
      {state:"unavailable",requestId:"ui-4",query:"",entries:[],category:"relay said no"},
      {state:"snapshot",requestId:"ui-4",query:"",entries:[],category:"people_timeout"},
      {state:"snapshot",requestId:"bad id",query:"",entries:[],category:null},
      {state:"snapshot",requestId:"ui-4",query:"a\u202eb",entries:[],category:null},
      {state:"snapshot",requestId:"ui-4",query:"x".repeat(65),entries:[],category:null},
      {state:"snapshot",requestId:"ui-4",query:"",entries:[{key:"D".repeat(64),name:"Dana"}],category:null},
      {state:"snapshot",requestId:"ui-4",query:"",entries:[entry,entry],category:null},
      {state:"snapshot",requestId:"ui-4",query:"",entries:[{key:dana,name:"Da\nna"}],category:null},
      {state:"snapshot",requestId:"ui-4",query:"",entries:[{key:dana,name:"é".repeat(33)}],category:null},
      {state:"snapshot",requestId:"ui-4",query:"",entries:Array.from({length: 51}, function(_, n) { return {key: (n + 16).toString(16).repeat(32), name: "P"} }),category:null}
    ]
    peopleOk.forEach(function(value, index) {
      offline.beginSession()
      if (!offline.acceptFrame(frame(withPeople, idle, value)) || !offline.peopleSupported)
        throw new Error("Valid people view " + index + " was rejected")
    })
    peopleBad.forEach(function(value, index) {
      offline.beginSession()
      if (offline.acceptFrame(frame(withPeople, idle, value)) || !offline.sessionFailed || offline.category !== "invalid_response")
        throw new Error("Malformed people view " + index + " was accepted")
    })
    // Served keys are chosen only after the helper served them; self never.
    offline.beginSession()
    if (!offline.acceptFrame(frame(withPeople, idle, peopleOk[0])) || offline.toggleDmParticipant(dana))
      throw new Error("A key nobody served was selectable")
    offline.searchPeople("")
    var asked = offline.peopleRequestId
    if (!offline.acceptFrame(frame(withPeople, idle, {state:"snapshot",requestId:asked || "ui-9",query:"",entries:[entry],category:null})))
      throw new Error("People snapshot was rejected")
    if (!offline.dmKeyAllowed(dana) || dana === offline.identity || offline.dmKeyAllowed(self)) throw new Error("Served keys are not what is allowed")
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
        if (test.ticks > 280) { console.error("New DM fixture timed out at stage " + test.stage); Qt.exit(1); return }
        if (test.stage === -1) { service.retry(); test.stage = 0; return }
        if (test.stage === 0 && service.selectedRoomId === test.room && service.recipientsState === "snapshot") {
          var control = test.shown(view, "buzzNewDm")
          if (control.length !== 1) throw new Error("New message control not shown")
          if (test.shown(view, "buzzNewDmPicker").length) throw new Error("Picker open before it was asked for")
          control[0].clicked()
          if (test.shown(view, "buzzNewDmPicker").length !== 1) throw new Error("Picker did not open")
          if (test.shown(view, "buzzNewDmSearch").length !== 1) throw new Error("Search field not shown")
          test.stage = 10
        } else if (test.stage === 10 && service.peopleStatus === "ready") {
          // The relay's directory, as the helper served it, without the viewer.
          var candidates = test.candidates()
          if (candidates.length !== 10 || candidates[0].key !== test.member)
            throw new Error("Picker must list the directory: " + candidates.length)
          if (candidates[0].text.indexOf("Synthetic person") === -1 || candidates[0].text.indexOf("cccccccc") === -1
              || candidates[0].text.indexOf("Participant") === -1)
            throw new Error("Candidate lacks name, key prefix or participant label: " + candidates[0].text)
          if (test.note() !== "") throw new Error("A note was shown over a listed directory: " + test.note())
          var start = test.shown(view, "buzzNewDmStart")[0]
          if (start.enabled) throw new Error("Start enabled with nobody chosen")
          candidates[0].clicked()
          if (JSON.stringify(service.dmSelection) !== JSON.stringify([test.member]) || !start.enabled)
            throw new Error("Choosing a person did not enable Start")
          if (test.shown(view, "buzzNewDmChip").length !== 1 || test.candidates().length !== 9)
            throw new Error("A chosen person must become a chip and leave the list")
          // Self and keys from nowhere are refused even when asked directly.
          if (service.openDm([test.self]) || service.openDm(["d".repeat(64).replace(/d/g, "9")]) || service.openDm([])
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
          if (seen !== "fetch_recent 11111111,fetch_recipients 11111111,search_people:,open_dm,room_check_listed_dm,fetch_recent 44444444,fetch_recipients 44444444")
            return
          test.validationCases()
          // A second session: the existing conversation comes first.
          test.shown(view, "buzzNewDm")[0].clicked()
          if (test.shown(view, "buzzNewDmPicker").length !== 1) throw new Error("Picker did not reopen")
          test.stage = 4
        } else if (test.stage === 4 && service.peopleStatus === "ready") {
          var listed = test.candidates()
          if (listed.length !== 10 || listed[0].key !== test.member || listed[1].key !== test.dana)
            throw new Error("Existing partner first, then the directory, once each: " + listed.map(function(c) { return c.key.slice(0, 2) }).join(","))
          // Typing is debounced: four edits in one tick make one request.
          ;["d", "da", "dan", "dana"].forEach(function(text) { test.type(text) })
          test.stage = 5
        } else if (test.stage === 5 && service.peopleText === "dana" && service.peopleStatus === "ready") {
          var found = test.candidates()
          if (found.length !== 1 || found[0].key !== test.dana)
            throw new Error("Search must list only the match: " + found.length)
          found[0].clicked()
          // A person from the directory, not the room, may be started with.
          if (JSON.stringify(service.dmSelection) !== JSON.stringify([test.dana]) || !service.canStartDm)
            throw new Error("A served person could not be chosen")
          if (test.shown(view, "buzzNewDmSearch")[0].text !== "") throw new Error("Search was not cleared after a pick")
          test.stage = 6
        } else if (test.stage === 6 && service.peopleText === "" && service.peopleStatus === "ready") {
          var rest = test.candidates()
          if (rest.length !== 9 || rest.some(function(c) { return c.key === test.dana }))
            throw new Error("Back to the directory without the chosen person: " + rest.length)
          for (var n = 0; n < 7; n++) test.candidates()[0].clicked()
          if (service.dmSelection.length !== 8 || !service.canStartDm) throw new Error("Eight people could not be chosen")
          var ninth = test.candidates()
          if (ninth.length !== 2) throw new Error("Unexpected list at the limit: " + ninth.length)
          ninth[0].clicked()
          if (service.dmSelection.length !== 8 || test.shown(view, "buzzNewDmChip").length !== 8)
            throw new Error("A ninth person was chosen")
          test.shown(view, "buzzNewDmChip")[0].clicked()
          if (service.dmSelection.length !== 7 || test.candidates().length !== 3)
            throw new Error("Removing a chip did not return the person to the list")
          test.type("fail")
          test.stage = 7
        } else if (test.stage === 7 && service.peopleText === "fail" && service.peopleStatus === "failed") {
          if (test.note() !== "People search timed out" || test.shown(view, "buzzNewDmRetry").length !== 1)
            throw new Error("A failed read needs its reason and a retry: " + test.note())
          test.shown(view, "buzzNewDmRetry")[0].clicked()
          test.stage = 8
        } else if (test.stage === 8 && service.peopleStatus === "ready" && service.peopleText === "fail") {
          if (test.note().indexOf("No people match") !== 0 || test.note().indexOf("fail") === -1 || test.shown(view, "buzzNewDmRetry").length)
            throw new Error("An empty answer must say so: " + test.note())
          test.stage = 9
        } else if (test.stage === 9) {
          record.reload()
          var queries = JSON.parse(record.text()).requests.filter(function(r) { return r.indexOf("search_people:") === 0 })
          // The fixture records asynchronously; four edits in one tick were one search.
          if (queries.length < 6) return
          if (queries.join("|") !== "search_people:|search_people:|search_people:dana|search_people:|search_people:fail|search_people:fail")
            throw new Error("Unexpected searches: " + queries.join("|"))
          console.log("PASS: New message lists existing partners then the directory, searches with debounce, picks up to 8 as chips and clears the search, shows loading, failed and no-match states, and refuses self, foreign keys, malformed views and a missing capability")
          Qt.quit()
        }
      } catch (error) { console.error(error.message); Qt.exit(1) }
    }
  }
}
