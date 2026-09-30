// Synthetic status frames only; no helper, relay or identity access.
import QtQuick
import Quickshell
import qs.Commons
import "plugin" as Buzz

ShellRoot {
  id: test
  Buzz.Service { id: service; autoConnect: false }
  FloatingWindow {
    visible: true
    implicitWidth: 900
    implicitHeight: 520
    Buzz.PanelContent { id: view; anchors.fill: parent; service: service }
  }
  function findNamed(item, name, found) {
    if (item.objectName === name) found.push(item)
    for (var i = 0; i < item.children.length; i++) findNamed(item.children[i], name, found)
    return found
  }
  function header() { return test.findNamed(view, "buzzHistoryLabel", [])[0] }
  Timer {
    id: checkRendered
    interval: 100
    onTriggered: {
      try {
        var label = test.header()
        if (!label || !label.visible || label.text.indexOf("Live · 1 message shown") !== 0)
          throw new Error("Room header does not show the live label: " + (label ? label.text : "missing"))
        console.log("PASS: a primed live subscription reads Live and slows the thread refresh to 30 s; snapshots, refreshes and older helpers keep the auto-refreshing label and 8 s; malformed or unannounced live flags are refused")
        Qt.quit()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  Timer {
    interval: 100
    running: true
    onTriggered: {
      try {
        var room = "11111111-1111-4111-8111-111111111111"
        var rootId = "1".repeat(64)
        var caps = ["connection_status","room_catalog","room_history","history_auto_refresh","thread_replies","live_updates"]
        function row(id) { return {id:id,author:"a".repeat(64),time:100,text:"Synthetic message",edited:false,truncated:false,unavailable:false} }
        function history(live) {
          var value = {state:"snapshot",roomId:room,rows:[row(rootId)],hasMore:false,category:"history_completeness_unknown",nextCursor:null,olderState:"idle"}
          if (live !== undefined) value.live = live
          return value
        }
        function frame(live, capabilities) {
          return {version:1,type:service.instanceId === "" ? "hello" : "status",instanceId:"live-fixture",generation:1,
            capabilities:capabilities || caps,
            status:{generation:1,connection:"authenticated",category:null,identity:"b".repeat(64),relay:"wss://fixture.example/",
              catalog:{state:"partial",category:"room_catalog_partial",rooms:[{id:room,name:"Fixture",description:"",kind:"stream",participants:[],hidden:false}]},
              history:history(live),
              thread:{state:"snapshot",roomId:room,rootId:rootId,rows:[],hasMore:false,category:"thread_completeness_unknown"}}}
        }
        function accept(value) { if (!service.acceptFrame(JSON.stringify(value))) throw new Error("Valid live frame rejected") }
        // Field validation.
        var legacy = service.validatedHistory(history())
        if (!legacy || legacy.live !== false) throw new Error("A helper without live updates was not read as not live")
        if (!service.validatedHistory(history(true)) || service.validatedHistory(history(true)).live !== true)
          throw new Error("A live snapshot was rejected")
        ;["yes", 1, null, {}].forEach(function(value) {
          if (service.validatedHistory(history(value))) throw new Error("Malformed live flag accepted: " + JSON.stringify(value))
        })
        if (service.validatedHistory({state:"loading",roomId:room,rows:[],hasMore:null,category:null,live:true}))
          throw new Error("A loading view claimed to be live")
        if (!service.validCapabilities(caps.concat(["room_recipients","message_send","thread_send","room_activity","agent_profiles","thread_summaries","dm_open","older_history"])))
          throw new Error("The full 14-capability list was rejected")
        // Not live: the auto-refreshing label and the 8-second thread refresh.
        service.beginSession()
        accept(frame(false))
        if (!service.liveUpdatesSupported || service.historyLive) throw new Error("Live state misread")
        if (service.historyLabel.indexOf("Auto-refreshing snapshot · 1 message shown") !== 0) throw new Error("Unexpected label: " + service.historyLabel)
        if (service.threadRefreshInterval !== 8000) throw new Error("Thread refresh not 8 s without live")
        service.openThread(rootId)
        accept(frame(false))
        // Live: the label and the 30-second refresh.
        accept(frame(true))
        if (!service.historyLive || service.historyLabel.indexOf("Live · 1 message shown · completeness unknown") !== 0)
          throw new Error("Live label missing: " + service.historyLabel)
        if (service.threadRefreshInterval !== 30000) throw new Error("Thread refresh not slowed while live")
        // A refresh keeps the rows but not the live claim.
        service.refreshHistory()
        var loading = frame(false)
        loading.status.history = {state:"loading",roomId:room,rows:[],hasMore:null,category:null}
        loading.status.thread = {state:"unavailable",roomId:null,rootId:null,rows:[],hasMore:null,category:null}
        accept(loading)
        if (service.historyState !== "snapshot" || service.historyLive || service.threadRefreshInterval !== 8000)
          throw new Error("Refresh kept the live claim")
        accept(frame(false))
        if (service.historyLive || service.historyLabel.indexOf("Auto-refreshing snapshot") !== 0) throw new Error("Subscription loss kept Live")
        accept(frame(true))
        if (!service.historyLive) throw new Error("Re-armed subscription not shown")
        checkRendered.start()
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
  // An unannounced live flag is an incompatible helper (checked on a separate service).
  Buzz.Service { id: strict; autoConnect: false }
  Timer {
    interval: 50
    running: true
    onTriggered: {
      try {
        var room = "22222222-2222-4222-8222-222222222222"
        var value = {version:1,type:"hello",instanceId:"live-strict",generation:1,
          capabilities:["connection_status","room_catalog","room_history"],
          status:{generation:1,connection:"authenticated",category:null,identity:"b".repeat(64),relay:"wss://fixture.example/",
            catalog:{state:"partial",category:"room_catalog_partial",rooms:[{id:room,name:"Fixture",description:"",kind:"stream",participants:[],hidden:false}]},
            history:{state:"snapshot",roomId:room,rows:[],hasMore:false,category:"history_completeness_unknown",live:true}}}
        strict.beginSession()
        if (strict.acceptFrame(JSON.stringify(value))) throw new Error("Live flag accepted without the live_updates capability")
      } catch (error) { console.error(error); Qt.exit(1) }
    }
  }
}
