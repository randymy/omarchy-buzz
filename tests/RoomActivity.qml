// Offscreen service boundary check with synthetic helper frames and fake notifications.
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  Buzz.Service { id: service; autoConnect: false }
  Timer {
    id: finish
    interval: 450
    onTriggered: { console.log("Buzz offscreen room activity check passed"); Qt.quit() }
  }
  Timer {
    interval: 100
    running: true
    onTriggered: {
      try {
        var roomA = "11111111-1111-4111-8111-111111111111"
        var roomB = "22222222-2222-4222-8222-222222222222"
        var self = "a".repeat(64)
        var other = "b".repeat(64)
        var caps = ["connection_status", "room_catalog", "room_history", "room_activity"]
        function entry(roomId, observed, epoch) { return {roomId:roomId, observed:observed, epoch:epoch || 1} }
        function frame(type, activity, options) {
          options = options || {}
          var loading = options.loading === true
          var rooms = options.revoked ? [roomA] : [roomA, roomB]
          var catalogRooms = loading ? [] : rooms.map(function(id) {
            return {id:id, name:id === roomA ? "First" : "Second", description:"",kind:"stream",participants:[],hidden:false}
          })
          var history = loading ? {state:"unavailable",roomId:null,rows:[],hasMore:null,category:null}
            : {state:"snapshot",roomId:roomA,rows:[],hasMore:false,category:"history_completeness_unknown"}
          var status = {generation:options.generation || 1,connection:"authenticated",category:null,
            identity:self,relay:options.relay || "wss://fixture.example/",
            catalog:{state:loading ? "loading" : "partial",category:loading ? null : "room_catalog_partial",rooms:catalogRooms},
            history:history,activity:activity}
          if (options.recipients) status.recipients = options.recipients
          return {version:1,type:type,instanceId:"room-activity-fixture",generation:status.generation,
            capabilities:options.capabilities || caps,status:status}
        }
        function accept(type, activity, options) {
          if (!service.acceptFrame(JSON.stringify(frame(type, activity, options)))) throw new Error("Valid room activity frame rejected")
        }
        function counts(total, first, second) {
          if (service.observedActivityCount !== total || service.roomActivityCount(roomA) !== first
              || service.roomActivityCount(roomB) !== second)
            throw new Error("Unexpected room counts: " + service.observedActivityCount + ", "
              + service.roomActivityCount(roomA) + ", " + service.roomActivityCount(roomB))
        }

        service.beginSession()
        service.notificationsEnabled = true
        accept("hello", [entry(roomA, 5), entry(roomB, 2)])
        counts(0, 0, 0)
        accept("status", [entry(roomA, 6), entry(roomB, 3)])
        counts(2, 1, 1)
        accept("status", [entry(roomA, 6), entry(roomB, 3)])
        counts(2, 1, 1)

        service.panelOpen = true
        counts(1, 0, 1)
        accept("status", [entry(roomA, 7), entry(roomB, 4)])
        counts(2, 0, 2)
        service.panelOpen = false
        // The periodic joined-room check does not blank the displayed counts.
        accept("status", [], {loading:true})
        counts(2, 0, 2)
        if (service.roomActivity.rooms[roomB].seen !== 2) throw new Error("Loading discarded observed state")
        accept("status", [entry(roomA, 7), entry(roomB, 4)])
        counts(2, 0, 2)
        accept("status", [entry(roomA, 7)], {revoked:true})
        counts(0, 0, 0)
        accept("status", [entry(roomA, 20)], {revoked:true,generation:2})
        counts(0, 0, 0)
        accept("status", [entry(roomA, 21)], {revoked:true,generation:2})
        counts(1, 1, 0)

        var forged = frame("status", [entry(roomA, 21)], {revoked:true,generation:2,
          capabilities:caps.concat(["room_recipients", "agent_profiles"]),
          recipients:{state:"snapshot",roomId:roomA,partial:false,category:null,
            entries:[{key:self,name:"Human"}],agents:[{key:other,name:"Forged agent",
              profileEventId:"c".repeat(64),executionState:"unknown"}]}})
        if (service.acceptFrame(JSON.stringify(forged)) || !service.sessionFailed)
          throw new Error("Agent absent from verified room roster was accepted")
        finish.start()
      } catch (error) {
        console.error("Buzz offscreen room activity check failed: " + error)
        Qt.exit(1)
      }
    }
  }
}
