// Offscreen boundary check using synthetic status frames and no helper connection.
import QtQuick
import Quickshell
import "plugin" as Buzz

ShellRoot {
  Buzz.Service { id: service; autoConnect: false }
  Timer {
    id: finish
    interval: 350
    onTriggered: {
      console.log("Buzz offscreen activity boundary check passed")
      Qt.quit()
    }
  }

  Timer {
    interval: 100
    running: true
    onTriggered: {
      try {
        var room = "11111111-1111-4111-8111-111111111111"
        var self = "a".repeat(64)
        var other = "b".repeat(64)
        var now = Math.floor(Date.now() / 1000)
        var caps = ["connection_status", "room_catalog", "room_history", "history_auto_refresh"]
        function row(id, author) {
          return {id:id.repeat(64), author:author, time:now, text:"synthetic", edited:false,
            truncated:false, unavailable:false}
        }
        function frame(type, rows, options) {
          options = options || {}
          var active = options.connection === undefined ? "authenticated" : options.connection
          var catalogState = options.catalog === undefined ? "partial" : options.catalog
          return {version:1, type:type, instanceId:options.instance || "activity-fixture",
            generation:options.generation || 1, capabilities:caps,
            status:{generation:options.generation || 1, connection:active, category:null,
              identity:options.identity === undefined ? self : options.identity,
              relay:options.relay || "wss://fixture.example/",
              catalog:{state:catalogState, category:catalogState === "partial" ? "room_catalog_partial" : null,
                rooms:catalogState === "partial" ? [{id:room,name:"Synthetic Room",description:"",kind:"stream",participants:[],hidden:false}] : []},
              history:{state:active === "authenticated" && catalogState === "partial" ? "snapshot" : "unavailable",
                roomId:active === "authenticated" && catalogState === "partial" ? room : null,
                rows:active === "authenticated" && catalogState === "partial" ? rows : [],
                hasMore:active === "authenticated" && catalogState === "partial" ? false : null,
                category:active === "authenticated" && catalogState === "partial" ? "history_completeness_unknown" : null}}}
        }
        function accept(type, rows, options) {
          if (!service.acceptFrame(JSON.stringify(frame(type, rows, options)))) throw new Error("Valid activity frame rejected")
        }
        function idsAre(expected) {
          if (service.activityObservation.ids.join(",") !== expected.join(","))
            throw new Error("Unexpected observed IDs: " + service.activityObservation.ids.join(","))
        }

        service.beginSession()
        service.notificationsEnabled = true
        service.panelOpen = true
        accept("hello", [row("1", other)])
        idsAre(["1".repeat(64)])
        accept("status", [row("1", other), row("2", other)])
        idsAre(["1".repeat(64), "2".repeat(64)])
        if (service.historyRows.length !== 2 || !service.panelOpen) throw new Error("Panel-open observation failed")

        // Invalid history must fail before any new row can enter the observer.
        var malformed = frame("status", [row("3", other)])
        malformed.status.history.rows[0].id = "invalid"
        if (service.acceptFrame(JSON.stringify(malformed)) || !service.sessionFailed
            || service.activityObservation.scope !== "") throw new Error("Malformed frame caused activity")

        service.beginSession()
        accept("hello", [row("1", other)])
        service.clearHistory()
        if (service.activityObservation.scope !== "" || service.historyRows.length)
          throw new Error("History clear retained a notification baseline")
        accept("status", [row("1", other), row("2", other)])
        idsAre(["1".repeat(64), "2".repeat(64)])

        // The periodic joined-room check keeps the same-scope conversation and its baseline.
        accept("status", [], {catalog:"loading"})
        if (service.activityObservation.scope === "" || service.historyRows.length !== 2)
          throw new Error("Joined-room check discarded the displayed conversation")
        accept("status", [row("2", other), row("3", other)])
        idsAre(["1".repeat(64), "2".repeat(64), "3".repeat(64)])

        accept("status", [], {connection:"connecting", catalog:"unavailable"})
        if (service.activityObservation.scope !== "" || service.historyRows.length)
          throw new Error("Reconnect retained observed history")
        accept("status", [row("3", other), row("4", other)])
        idsAre(["3".repeat(64), "4".repeat(64)])

        var before = service.activityObservation.scope
        accept("status", [row("4", other), row("5", other)], {relay:"wss://another.example/"})
        if (service.activityObservation.scope === before) throw new Error("Relay scope did not change")
        idsAre(["4".repeat(64), "5".repeat(64)])

        // Authenticated frames without an identity may display history but cannot observe activity.
        before = service.activityObservation.scope
        accept("status", [row("5", other), row("6", self)], {relay:"wss://another.example/", identity:null})
        if (service.activityObservation.scope !== before) throw new Error("Null identity observed activity")
        idsAre(["4".repeat(64), "5".repeat(64)])

        service.notificationsEnabled = false
        service.notificationsEnabled = true
        service.panelOpen = false
        accept("status", [row("5", other)], {relay:"wss://another.example/"})
        idsAre(["5".repeat(64)])
        accept("status", [row("5", other), row("6", self)], {relay:"wss://another.example/"})
        idsAre(["5".repeat(64), "6".repeat(64)])
        if (Quickshell.env("BUZZ_ACTIVITY_POSITIVE")) {
          accept("status", [row("5", other), row("6", self), row("7", other)], {relay:"wss://another.example/"})
          accept("status", [row("5", other), row("6", self), row("7", other), row("8", other)], {relay:"wss://another.example/"})
          idsAre(["5".repeat(64), "6".repeat(64), "7".repeat(64), "8".repeat(64)])
        }
        finish.start()
      } catch (error) {
        console.error("Buzz offscreen activity boundary check failed: " + error)
        Qt.exit(1)
      }
    }
  }
}
